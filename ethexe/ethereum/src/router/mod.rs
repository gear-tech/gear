// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
    abi::{utils::uint256_to_u256, Gear::CodeState, IRouter},
    wvara::WVara,
    AlloyEthereum, AlloyProvider, TryGetReceipt,
};
use alloy::{
    consensus::{SidecarBuilder, SimpleCoder},
    primitives::{fixed_bytes, Address, Bytes, B256, U256},
    providers::{PendingTransactionBuilder, Provider, ProviderBuilder, RootProvider},
    rpc::types::{eth::state::AccountOverride, Filter},
};
use anyhow::{anyhow, Result};
use ethexe_common::gear::{AggregatedPublicKey, BatchCommitment, SignatureType};
use ethexe_signer::{Address as LocalAddress, ContractSignature};
use events::signatures;
use futures::StreamExt;
use gear_core::ids::{prelude::CodeIdExt as _, ProgramId};
use gprimitives::{ActorId, CodeId, H256};
use std::collections::HashMap;

pub mod events;

type Instance = IRouter::IRouterInstance<(), AlloyProvider>;
type QueryInstance = IRouter::IRouterInstance<(), RootProvider>;

pub struct PendingCodeRequestBuilder {
    code_id: CodeId,
    pending_builder: PendingTransactionBuilder<AlloyEthereum>,
}

impl PendingCodeRequestBuilder {
    pub fn code_id(&self) -> CodeId {
        self.code_id
    }

    pub fn tx_hash(&self) -> H256 {
        H256(self.pending_builder.tx_hash().0)
    }

    pub async fn send(self) -> Result<(H256, CodeId)> {
        let receipt = self.pending_builder.try_get_receipt().await?;
        Ok(((*receipt.transaction_hash).into(), self.code_id))
    }
}

#[derive(Clone)]
pub struct Router {
    instance: Instance,
    wvara_address: Address,
}

impl Router {
    /// `Gear.blockIsPredecessor(hash)` can consume up to 30_000 gas
    const GEAR_BLOCK_IS_PREDECESSOR_GAS: u64 = 30_000;
    /// Huge gas limit is necessary so that the transaction is more likely to be picked up
    const HUGE_GAS_LIMIT: u64 = 10_000_000;

    pub(crate) fn new(address: Address, wvara_address: Address, provider: AlloyProvider) -> Self {
        Self {
            instance: Instance::new(address, provider),
            wvara_address,
        }
    }

    pub fn address(&self) -> LocalAddress {
        LocalAddress(*self.instance.address().0)
    }

    pub fn query(&self) -> RouterQuery {
        RouterQuery {
            instance: QueryInstance::new(
                *self.instance.address(),
                self.instance.provider().root().clone(),
            ),
        }
    }

    pub fn wvara(&self) -> WVara {
        WVara::new(self.wvara_address, self.instance.provider().clone())
    }

    pub async fn request_code_validation_with_sidecar(
        &self,
        code: &[u8],
    ) -> Result<PendingCodeRequestBuilder> {
        let code_id = CodeId::generate(code);

        let builder = self
            .instance
            .requestCodeValidation(code_id.into_bytes().into())
            .sidecar(SidecarBuilder::<SimpleCoder>::from_slice(code).build()?);
        let pending_builder = builder.send().await?;

        Ok(PendingCodeRequestBuilder {
            code_id,
            pending_builder,
        })
    }

    pub async fn wait_code_validation(&self, code_id: CodeId) -> Result<bool> {
        let filter = Filter::new().address(*self.instance.address());
        let mut router_events = self
            .instance
            .provider()
            .subscribe_logs(&filter)
            .await?
            .into_stream();

        let code_id = code_id.into_bytes();

        while let Some(log) = router_events.next().await {
            if let Some(signatures::CODE_GOT_VALIDATED) = log.topic0().cloned() {
                let event = crate::decode_log::<IRouter::CodeGotValidated>(&log)?;

                if event.codeId == code_id {
                    return Ok(event.valid);
                }
            }
        }

        Err(anyhow!("Failed to define if code is validated"))
    }

    pub async fn create_program(&self, code_id: CodeId, salt: H256) -> Result<(H256, ActorId)> {
        let builder = self.instance.createProgram(
            code_id.into_bytes().into(),
            salt.to_fixed_bytes().into(),
            Address::ZERO,
        );
        let receipt = builder.send().await?.try_get_receipt().await?;

        let tx_hash = (*receipt.transaction_hash).into();
        let mut actor_id = None;

        for log in receipt.inner.logs() {
            if log.topic0().cloned() == Some(signatures::PROGRAM_CREATED) {
                let event = crate::decode_log::<IRouter::ProgramCreated>(log)?;

                actor_id = Some((*event.actorId.into_word()).into());

                break;
            }
        }

        let actor_id = actor_id.ok_or_else(|| anyhow!("Couldn't find `ProgramCreated` log"))?;

        Ok((tx_hash, actor_id))
    }

    pub async fn commit_batch(
        &self,
        commitment: BatchCommitment,
        signatures: Vec<ContractSignature>,
    ) -> Result<H256> {
        let builder = self.instance.commitBatch(
            commitment.into(),
            SignatureType::ECDSA as u8,
            signatures
                .into_iter()
                .map(|signature| Bytes::copy_from_slice(signature.as_ref()))
                .collect(),
        );

        let mut state_diff = HashMap::default();
        state_diff.insert(
            // keccak256(abi.encode(uint256(keccak256(bytes("router.storage.RouterV1"))) - 1)) & ~bytes32(uint256(0xff))
            fixed_bytes!("e3d827fd4fed52666d49a0df00f9cc2ac79f0f2378fc627e62463164801b6500"),
            // router.reserved = 1
            fixed_bytes!("0000000000000000000000000000000000000000000000000000000000000001"),
        );

        let mut state = HashMap::default();
        state.insert(
            *self.instance.address(),
            AccountOverride {
                state_diff: Some(state_diff),
                ..Default::default()
            },
        );

        let estimate_gas_builder = builder.clone().state(state);
        let gas_limit = Self::HUGE_GAS_LIMIT
            .max(estimate_gas_builder.estimate_gas().await? + Self::GEAR_BLOCK_IS_PREDECESSOR_GAS);

        let receipt = builder
            .gas(gas_limit)
            .send()
            .await?
            .try_get_receipt()
            .await?;
        Ok(H256(receipt.transaction_hash.0))
    }
}

#[derive(Clone)]
pub struct RouterQuery {
    instance: QueryInstance,
}

impl RouterQuery {
    pub async fn new(rpc_url: &str, router_address: LocalAddress) -> Result<Self> {
        let provider = ProviderBuilder::default().connect(rpc_url).await?;

        Ok(Self {
            instance: QueryInstance::new(Address::new(router_address.0), provider),
        })
    }

    pub fn from_provider(router_address: Address, provider: RootProvider) -> Self {
        Self {
            instance: QueryInstance::new(router_address, provider),
        }
    }

    pub async fn genesis_block_hash(&self) -> Result<H256> {
        self.instance
            .genesisBlockHash()
            .call()
            .await
            .map(|res| H256(*res._0))
            .map_err(Into::into)
    }

    pub async fn latest_committed_block_hash(&self) -> Result<H256> {
        self.instance
            .latestCommittedBlockHash()
            .call()
            .await
            .map(|res| H256(*res._0))
            .map_err(Into::into)
    }

    pub async fn mirror_impl(&self) -> Result<LocalAddress> {
        self.instance
            .mirrorImpl()
            .call()
            .await
            .map(|res| LocalAddress(res._0.into()))
            .map_err(Into::into)
    }

    pub async fn wvara_address(&self) -> Result<Address> {
        self.instance
            .wrappedVara()
            .call()
            .await
            .map(|res| res._0)
            .map_err(Into::into)
    }

    pub async fn validators_aggregated_public_key(&self) -> Result<AggregatedPublicKey> {
        self.instance
            .validatorsAggregatedPublicKey()
            .call()
            .await
            .map(|res| AggregatedPublicKey {
                x: uint256_to_u256(res._0.x),
                y: uint256_to_u256(res._0.y),
            })
            .map_err(Into::into)
    }

    pub async fn validators_verifiable_secret_sharing_commitment(&self) -> Result<Vec<u8>> {
        self.instance
            .validatorsVerifiableSecretSharingCommitment()
            .call()
            .await
            .map(|res| res._0.into())
            .map_err(Into::into)
    }

    pub async fn validators(&self) -> Result<Vec<LocalAddress>> {
        self.instance
            .validators()
            .call()
            .await
            .map(|res| res._0.into_iter().map(|v| LocalAddress(v.into())).collect())
            .map_err(Into::into)
    }

    pub async fn validators_at(&self, block: H256) -> Result<Vec<LocalAddress>> {
        self.instance
            .validators()
            .call()
            .block(B256::from(block.0).into())
            .await
            .map(|res| res._0.into_iter().map(|v| LocalAddress(v.into())).collect())
            .map_err(Into::into)
    }

    pub async fn threshold(&self) -> Result<u64> {
        self.instance
            .validatorsThreshold()
            .call()
            .await
            .map(|res| res._0.to())
            .map_err(Into::into)
    }

    pub async fn signing_threshold_percentage(&self) -> Result<u16> {
        self.instance
            .signingThresholdPercentage()
            .call()
            .await
            .map(|res| res._0)
            .map_err(Into::into)
    }

    pub async fn code_state(&self, code_id: CodeId) -> Result<CodeState> {
        self.instance
            .codeState(code_id.into_bytes().into())
            .call()
            .await
            .map(|res| CodeState::from(res._0))
            .map_err(Into::into)
    }

    pub async fn codes_states(&self, code_ids: Vec<CodeId>) -> Result<Vec<CodeState>> {
        self.instance
            .codesStates(
                code_ids
                    .into_iter()
                    .map(|c| c.into_bytes().into())
                    .collect(),
            )
            .call()
            .await
            .map(|res| res._0.into_iter().map(CodeState::from).collect())
            .map_err(Into::into)
    }

    pub async fn program_code_id(&self, program_id: ProgramId) -> Result<Option<CodeId>> {
        let program_id = LocalAddress::try_from(program_id).expect("infallible");
        let program_id = Address::new(program_id.0);
        let code_id = self.instance.programCodeId(program_id).call().await?;
        let code_id = Some(CodeId::new(code_id._0.0)).filter(|&code_id| code_id != CodeId::zero());
        Ok(code_id)
    }

    pub async fn programs_code_ids(&self, program_ids: Vec<ProgramId>) -> Result<Vec<CodeId>> {
        self.instance
            .programsCodeIds(
                program_ids
                    .into_iter()
                    .map(|p| {
                        let program_id = LocalAddress::try_from(p).expect("infallible");
                        Address::new(program_id.0)
                    })
                    .collect(),
            )
            .call()
            .await
            .map(|res| res._0.into_iter().map(|c| CodeId::new(c.0)).collect())
            .map_err(Into::into)
    }

    pub async fn programs_count(&self) -> Result<U256> {
        let count = self.instance.programsCount().call().await?;
        Ok(count._0)
    }

    pub async fn validated_codes_count(&self) -> Result<U256> {
        let count = self.instance.validatedCodesCount().call().await?;
        Ok(count._0)
    }
}
