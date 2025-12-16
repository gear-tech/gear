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
    AlloyEthereum, AlloyProvider, IntoBlockId, TryGetReceipt,
    abi::{IRouter, Router::StorageView, utils::uint256_to_u256},
    wvara::WVara,
};
use alloy::{
    consensus::{SidecarBuilder, SimpleCoder},
    eips::eip7594::BlobTransactionSidecarVariant,
    primitives::{Address, Bytes, fixed_bytes},
    providers::{PendingTransactionBuilder, Provider, ProviderBuilder, RootProvider},
    rpc::types::{Filter, Topic, eth::state::AccountOverride},
};
use anyhow::{Result, anyhow};
use ethexe_common::{
    Address as LocalAddress, Digest, ValidatorsVec,
    ecdsa::ContractSignature,
    gear::{AggregatedPublicKey, BatchCommitment, CodeState, SignatureType, Timelines},
};
use events::signatures;
use futures::StreamExt;
use gear_core::ids::prelude::CodeIdExt as _;
use gprimitives::{ActorId, CodeId, H256};
use std::collections::HashMap;

pub mod events;

type Instance = IRouter::IRouterInstance<AlloyProvider>;
type QueryInstance = IRouter::IRouterInstance<RootProvider>;

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
        let receipt = self
            .pending_builder
            .try_get_receipt_check_reverted()
            .await?;
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

        let chain_id = self.instance.provider().get_chain_id().await?;
        let blob_tx_sidecar_variant = if chain_id == 31337 {
            BlobTransactionSidecarVariant::Eip4844(
                SidecarBuilder::<SimpleCoder>::from_slice(code).build()?,
            )
        } else {
            BlobTransactionSidecarVariant::Eip7594(
                SidecarBuilder::<SimpleCoder>::from_slice(code).build_7594()?,
            )
        };

        let builder = self
            .instance
            .requestCodeValidation(code_id.into_bytes().into())
            .sidecar(blob_tx_sidecar_variant);
        let pending_builder = builder.send().await?;

        Ok(PendingCodeRequestBuilder {
            code_id,
            pending_builder,
        })
    }

    pub async fn wait_code_validation(&self, code_id: CodeId) -> Result<bool> {
        let filter = Filter::new()
            .address(*self.instance.address())
            .event_signature(Topic::from_iter([signatures::CODE_GOT_VALIDATED]));
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

    pub async fn create_program(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
    ) -> Result<(H256, ActorId)> {
        let builder = self.instance.createProgram(
            code_id.into_bytes().into(),
            salt.to_fixed_bytes().into(),
            override_initializer
                .map(|initializer| {
                    let initializer = LocalAddress::try_from(initializer).expect("infallible");
                    Address::new(initializer.0)
                })
                .unwrap_or_default(),
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

    pub async fn create_program_with_abi_interface(
        &self,
        code_id: CodeId,
        salt: H256,
        override_initializer: Option<ActorId>,
        abi_interface: ActorId,
    ) -> Result<(H256, ActorId)> {
        let abi_interface = LocalAddress::try_from(abi_interface).expect("infallible");
        let abi_interface = Address::new(abi_interface.0);

        let builder = self.instance.createProgramWithAbiInterface(
            code_id.into_bytes().into(),
            salt.to_fixed_bytes().into(),
            override_initializer
                .map(|initializer| {
                    let initializer = LocalAddress::try_from(initializer).expect("infallible");
                    Address::new(initializer.0)
                })
                .unwrap_or_default(),
            abi_interface,
        );
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;

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
        self.commit_batch_pending(commitment, signatures)
            .await?
            .try_get_receipt_check_reverted()
            .await
            .map(|receipt| H256(receipt.transaction_hash.0))
    }

    pub async fn commit_batch_pending(
        &self,
        commitment: BatchCommitment,
        signatures: Vec<ContractSignature>,
    ) -> Result<PendingTransactionBuilder<AlloyEthereum>> {
        let builder = self.instance.commitBatch(
            commitment.into(),
            SignatureType::ECDSA as u8,
            signatures
                .into_iter()
                .map(|signature| Bytes::from(signature.into_pre_eip155_bytes()))
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

        builder.gas(gas_limit).send().await.map_err(Into::into)
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
            .map(|res| H256(*res))
            .map_err(Into::into)
    }

    pub async fn latest_committed_batch_hash(&self) -> Result<Digest> {
        self.instance
            .latestCommittedBatchHash()
            .call()
            .await
            .map(|res| Digest(res.0))
            .map_err(Into::into)
    }

    pub async fn mirror_impl(&self) -> Result<LocalAddress> {
        self.instance
            .mirrorImpl()
            .call()
            .await
            .map(|res| LocalAddress(res.into()))
            .map_err(Into::into)
    }

    pub async fn middleware(&self) -> Result<LocalAddress> {
        self.instance
            .middleware()
            .call()
            .await
            .map(|res| LocalAddress(res.into()))
            .map_err(Into::into)
    }

    pub async fn wvara_address(&self) -> Result<Address> {
        self.instance.wrappedVara().call().await.map_err(Into::into)
    }

    pub async fn middleware_address(&self) -> Result<Address> {
        self.instance.middleware().call().await.map_err(Into::into)
    }

    pub async fn validators_at(&self, id: impl IntoBlockId) -> Result<ValidatorsVec> {
        let validators: Vec<_> = self
            .instance
            .validators()
            .call()
            .block(id.into_block_id())
            .await
            .map(|res| res.into_iter().map(|v| LocalAddress(v.into())).collect())
            .map_err(Into::<anyhow::Error>::into)?;
        validators.try_into().map_err(Into::into)
    }

    pub async fn validators_aggregated_public_key(&self) -> Result<AggregatedPublicKey> {
        self.instance
            .validatorsAggregatedPublicKey()
            .call()
            .await
            .map(|res| AggregatedPublicKey {
                x: uint256_to_u256(res.x),
                y: uint256_to_u256(res.y),
            })
            .map_err(Into::into)
    }

    pub async fn validators_verifiable_secret_sharing_commitment(&self) -> Result<Vec<u8>> {
        self.instance
            .validatorsVerifiableSecretSharingCommitment()
            .call()
            .await
            .map(|res| res.into())
            .map_err(Into::into)
    }

    pub async fn threshold(&self) -> Result<u64> {
        self.instance
            .validatorsThreshold()
            .call()
            .await
            .map(|res| res.to())
            .map_err(Into::into)
    }

    pub async fn signing_threshold_percentage(&self) -> Result<u16> {
        self.instance
            .signingThresholdPercentage()
            .call()
            .await
            .map_err(Into::into)
    }

    pub async fn codes_states_at(
        &self,
        code_ids: impl IntoIterator<Item = CodeId>,
        id: impl IntoBlockId,
    ) -> Result<Vec<CodeState>> {
        self.instance
            .codesStates(
                code_ids
                    .into_iter()
                    .map(|c| c.into_bytes().into())
                    .collect(),
            )
            .call()
            .block(id.into_block_id())
            .await
            .map(|res| res.into_iter().map(CodeState::from).collect())
            .map_err(Into::into)
    }

    pub async fn program_code_id(&self, program_id: ActorId) -> Result<Option<CodeId>> {
        let program_id = LocalAddress::try_from(program_id).expect("infallible");
        let program_id = Address::new(program_id.0);
        let code_id = self.instance.programCodeId(program_id).call().await?;
        let code_id = Some(CodeId::new(code_id.0)).filter(|&code_id| code_id != CodeId::zero());
        Ok(code_id)
    }

    pub async fn programs_code_ids_at(
        &self,
        program_ids: impl IntoIterator<Item = ActorId>,
        id: impl IntoBlockId,
    ) -> Result<Vec<CodeId>> {
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
            .block(id.into_block_id())
            .await
            .map(|res| res.into_iter().map(|c| CodeId::new(c.0)).collect())
            .map_err(Into::into)
    }

    pub async fn timelines(&self) -> Result<Timelines> {
        self.instance
            .timelines()
            .call()
            .await
            .map(|res| res.into())
            .map_err(Into::into)
    }

    pub async fn programs_count_at(&self, id: impl IntoBlockId) -> Result<u64> {
        let count = self
            .instance
            .programsCount()
            .call()
            .block(id.into_block_id())
            .await?;
        // it's impossible to ever reach 18 quintillion programs (maximum of u64)
        let count: u64 = count.try_into().expect("infallible");
        Ok(count)
    }

    pub async fn validated_codes_count_at(&self, id: impl IntoBlockId) -> Result<u64> {
        let count = self
            .instance
            .validatedCodesCount()
            .call()
            .block(id.into_block_id())
            .await?;
        // it's impossible to ever reach 18 quintillion programs (maximum of u64)
        let count: u64 = count.try_into().expect("infallible");
        Ok(count)
    }

    pub async fn storage_view_at(&self, id: impl IntoBlockId) -> Result<StorageView> {
        self.instance
            .storageView()
            .call()
            .block(id.into_block_id())
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::EthereumDeployer;
    use alloy::{eips::BlockId, node_bindings::Anvil};
    use ethexe_signer::Signer;

    #[tokio::test]
    async fn inexistent_code_is_unknown() {
        let anvil = Anvil::new().spawn();

        let signer = Signer::memory();
        let alice = signer
            .storage_mut()
            .add_key(
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                    .parse()
                    .unwrap(),
            )
            .unwrap();

        let states =
            EthereumDeployer::new(anvil.endpoint_url().as_str(), signer, alice.to_address())
                .await
                .unwrap()
                .deploy()
                .await
                .unwrap()
                .router()
                .query()
                .codes_states_at([CodeId::new([0xfe; 32])], BlockId::latest())
                .await
                .unwrap();
        assert_eq!(states, vec![CodeState::Unknown]);
    }

    #[tokio::test]
    async fn storage_view() {
        let anvil = Anvil::new().spawn();

        let signer = Signer::memory();
        let alice = signer
            .storage_mut()
            .add_key(
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                    .parse()
                    .unwrap(),
            )
            .unwrap();

        let storage =
            EthereumDeployer::new(anvil.endpoint_url().as_str(), signer, alice.to_address())
                .await
                .unwrap()
                .deploy()
                .await
                .unwrap()
                .router()
                .query()
                .storage_view_at(BlockId::latest())
                .await
                .unwrap();
        assert!(storage.validationSettings.validators0.useFromTimestamp > 0);
        assert_eq!(storage.validationSettings.validators0.list.len(), 1);
        assert_eq!(storage.validationSettings.validators1.useFromTimestamp, 0);
    }
}
