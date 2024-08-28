// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::{abi::IRouter, wvara::WVara, AlloyProvider, AlloyTransport};
use alloy::{
    consensus::{SidecarBuilder, SimpleCoder},
    primitives::{Address, Bytes, B256},
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::Filter,
    transports::BoxTransport,
};
use anyhow::{anyhow, Result};
use ethexe_common::router::{BlockCommitment, CodeCommitment};
use ethexe_signer::{Address as LocalAddress, Signature as LocalSignature};
use events::signatures;
use futures::StreamExt;
use gear_core::ids::prelude::CodeIdExt as _;
use gprimitives::{ActorId, CodeId, H160, H256};
use std::sync::Arc;

pub mod events;

type InstanceProvider = Arc<AlloyProvider>;
type Instance = IRouter::IRouterInstance<AlloyTransport, InstanceProvider>;

type QueryInstance = IRouter::IRouterInstance<AlloyTransport, Arc<RootProvider<BoxTransport>>>;

pub struct Router {
    instance: Instance,
    wvara_address: Address,
}

impl Router {
    pub(crate) fn new(
        address: Address,
        wvara_address: Address,
        provider: InstanceProvider,
    ) -> Self {
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
                Arc::new(self.instance.provider().root().clone()),
            ),
        }
    }

    pub fn wvara(&self) -> WVara {
        WVara::new(self.wvara_address, self.instance.provider().clone())
    }

    pub async fn update_validators(&self, validators: Vec<H160>) -> Result<H256> {
        let validators = validators
            .into_iter()
            .map(|v| v.to_fixed_bytes().into())
            .collect();

        let builder = self.instance.updateValidators(validators);
        let tx = builder.send().await?;

        let receipt = tx.get_receipt().await?;

        Ok((*receipt.transaction_hash).into())
    }

    pub async fn request_code_validation(
        &self,
        code_id: CodeId,
        blob_tx_hash: H256,
    ) -> Result<H256> {
        let builder = self.instance.requestCodeValidation(
            code_id.into_bytes().into(),
            blob_tx_hash.to_fixed_bytes().into(),
        );
        let tx = builder.send().await?;

        let receipt = tx.get_receipt().await?;

        Ok((*receipt.transaction_hash).into())
    }

    pub async fn request_code_validation_with_sidecar(
        &self,
        code: &[u8],
    ) -> Result<(H256, CodeId)> {
        let code_id = CodeId::generate(code);

        let builder = self
            .instance
            .requestCodeValidation(code_id.into_bytes().into(), B256::ZERO)
            .sidecar(SidecarBuilder::<SimpleCoder>::from_slice(code).build()?);
        let tx = builder.send().await?;

        let receipt = tx.get_receipt().await?;

        Ok(((*receipt.transaction_hash).into(), code_id))
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
            match log.topic0().map(|v| H256(v.0)) {
                Some(b) if b == signatures::CODE_GOT_VALIDATED => {
                    let event = crate::decode_log::<IRouter::CodeGotValidated>(&log)?;

                    if event.id == code_id {
                        return Ok(event.valid);
                    }
                }
                _ => (),
            }
        }

        Err(anyhow::anyhow!("Failed to define if code is validated"))
    }

    pub async fn create_program(
        &self,
        code_id: CodeId,
        salt: H256,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(H256, ActorId)> {
        let builder = self.instance.createProgram(
            code_id.into_bytes().into(),
            salt.to_fixed_bytes().into(),
            payload.as_ref().to_vec().into(),
            value,
        );
        let tx = builder.send().await?;

        let receipt = tx.get_receipt().await?;

        let tx_hash = (*receipt.transaction_hash).into();
        let mut actor_id = None;

        // TODO: return init message id
        for log in receipt.inner.logs() {
            if log.topic0().map(|v| v.0) == Some(signatures::PROGRAM_CREATED.to_fixed_bytes()) {
                let event = crate::decode_log::<IRouter::ProgramCreated>(log)?;

                actor_id = Some((*event.actorId.into_word()).into());

                break;
            }
        }

        let actor_id = actor_id.ok_or_else(|| anyhow!("Couldn't find `ProgramCreated` log"))?;

        Ok((tx_hash, actor_id))
    }

    pub async fn commit_codes(
        &self,
        commitments: Vec<CodeCommitment>,
        signatures: Vec<LocalSignature>,
    ) -> Result<H256> {
        let builder = self.instance.commitCodes(
            commitments.into_iter().map(Into::into).collect(),
            signatures
                .into_iter()
                .map(|signature| Bytes::copy_from_slice(signature.as_ref()))
                .collect(),
        );
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }

    pub async fn commit_blocks(
        &self,
        commitments: Vec<BlockCommitment>,
        signatures: Vec<LocalSignature>,
    ) -> Result<H256> {
        let builder = self
            .instance
            .commitBlocks(
                commitments.into_iter().map(Into::into).collect(),
                signatures
                    .into_iter()
                    .map(|signature| Bytes::copy_from_slice(signature.as_ref()))
                    .collect(),
            )
            .gas(10_000_000);
        let tx = builder.send().await?;
        let receipt = tx.get_receipt().await?;
        Ok(H256(receipt.transaction_hash.0))
    }
}

#[derive(Clone)]
pub struct RouterQuery {
    instance: QueryInstance,
}

impl RouterQuery {
    pub async fn new(rpc_url: &str, router_address: LocalAddress) -> Result<Self> {
        let provider = Arc::new(ProviderBuilder::new().on_builtin(rpc_url).await?);

        Ok(Self {
            instance: QueryInstance::new(Address::new(router_address.0), provider),
        })
    }

    pub fn from_provider(
        router_address: Address,
        provider: Arc<RootProvider<BoxTransport>>,
    ) -> Self {
        Self {
            instance: QueryInstance::new(router_address, provider),
        }
    }

    pub async fn wvara_address(&self) -> Result<Address> {
        self.instance
            .wrappedVara()
            .call()
            .await
            .map(|res| res._0)
            .map_err(Into::into)
    }

    pub async fn last_commitment_block_hash(&self) -> Result<H256> {
        self.instance
            .lastBlockCommitmentHash()
            .call()
            .await
            .map(|res| H256(*res._0))
            .map_err(Into::into)
    }

    pub async fn genesis_block_hash(&self) -> Result<H256> {
        self.instance
            .genesisBlockHash()
            .call()
            .await
            .map(|res| H256(*res._0))
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

    pub async fn threshold(&self) -> Result<u64> {
        self.instance
            .validatorsThreshold()
            .call()
            .await
            .map(|res| res._0.to())
            .map_err(Into::into)
    }
}
