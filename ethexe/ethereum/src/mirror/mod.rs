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
    AlloyProvider, TryGetReceipt,
    abi::{self, IMirror},
};
use alloy::{
    contract::CallBuilder,
    eips::BlockId,
    network,
    primitives::{Address, Bytes, U256},
    providers::{PendingTransactionBuilder, Provider, RootProvider},
};
use anyhow::Result;
use ethexe_common::Address as LocalAddress;
pub use events::signatures;
use gprimitives::{H256, MessageId};

pub mod events;

type Instance = IMirror::IMirrorInstance<AlloyProvider>;
type QueryInstance = IMirror::IMirrorInstance<RootProvider>;

pub struct Mirror(Instance);

impl Mirror {
    pub(crate) fn new(address: Address, provider: AlloyProvider) -> Self {
        Self(Instance::new(address, provider))
    }

    pub fn address(&self) -> LocalAddress {
        LocalAddress(*self.0.address().0)
    }

    pub fn query(&self) -> MirrorQuery {
        MirrorQuery(QueryInstance::new(
            *self.0.address(),
            self.0.provider().root().clone(),
        ))
    }

    pub async fn get_balance(&self) -> Result<u128> {
        self.0
            .provider()
            .get_balance(*self.0.address())
            .await
            .map(abi::utils::uint256_to_u128_lossy)
            .map_err(Into::into)
    }

    pub async fn owned_balance_top_up(&self, value: u128) -> Result<H256> {
        let builder = CallBuilder::new_raw(self.0.provider(), Bytes::new())
            .to(*self.0.address())
            .value(U256::from(value));
        let receipt = builder.send().await?.try_get_receipt().await?;

        Ok((*receipt.transaction_hash).into())
    }

    pub async fn executable_balance_top_up(&self, value: u128) -> Result<H256> {
        let builder = self.0.executableBalanceTopUp(value);
        let receipt = builder.send().await?.try_get_receipt().await?;

        Ok((*receipt.transaction_hash).into())
    }

    pub async fn send_message(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
        call_reply: bool,
    ) -> Result<(H256, MessageId)> {
        self.send_message_pending(payload, value, call_reply)
            .await?
            .try_get_message_send_receipt()
            .await
    }

    pub async fn send_message_pending(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
        call_reply: bool,
    ) -> Result<PendingTransactionBuilder<network::Ethereum>> {
        self.0
            .sendMessage(payload.as_ref().to_vec().into(), call_reply)
            .value(U256::from(value))
            .send()
            .await
            .map_err(Into::into)
    }

    pub async fn send_reply(
        &self,
        replied_to: MessageId,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<H256> {
        let builder = self
            .0
            .sendReply(
                replied_to.into_bytes().into(),
                payload.as_ref().to_vec().into(),
            )
            .value(U256::from(value));
        let receipt = builder.send().await?.try_get_receipt().await?;

        Ok((*receipt.transaction_hash).into())
    }

    pub async fn claim_value(&self, claimed_id: MessageId) -> Result<H256> {
        let builder = self.0.claimValue(claimed_id.into_bytes().into());
        let receipt = builder.send().await?.try_get_receipt().await?;

        Ok((*receipt.transaction_hash).into())
    }
}

pub struct MirrorQuery(QueryInstance);

impl MirrorQuery {
    pub fn new(provider: RootProvider, mirror_address: LocalAddress) -> Self {
        Self(QueryInstance::new(Address::new(mirror_address.0), provider))
    }

    pub async fn router(&self) -> Result<LocalAddress> {
        self.0
            .router()
            .call()
            .await
            .map(|res| LocalAddress(res.into()))
            .map_err(Into::into)
    }

    pub async fn state_hash(&self) -> Result<H256> {
        self.0
            .stateHash()
            .call()
            .await
            .map(|res| H256(*res))
            .map_err(Into::into)
    }

    pub async fn state_hash_at(&self, block: H256) -> Result<H256> {
        self.0
            .stateHash()
            .block(BlockId::hash(block.0.into()))
            .call()
            .await
            .map(|res| H256(res.0))
            .map_err(Into::into)
    }

    pub async fn nonce(&self) -> Result<U256> {
        self.0
            .nonce()
            .call()
            .await
            .map(|res| U256::from(res))
            .map_err(Into::into)
    }

    pub async fn exited(&self) -> Result<bool> {
        self.0.exited().call().await.map_err(Into::into)
    }

    pub async fn inheritor(&self) -> Result<LocalAddress> {
        self.0
            .inheritor()
            .call()
            .await
            .map(|res| LocalAddress(res.into()))
            .map_err(Into::into)
    }

    pub async fn initializer(&self) -> Result<LocalAddress> {
        self.0
            .initializer()
            .call()
            .await
            .map(|res| LocalAddress(res.into()))
            .map_err(Into::into)
    }
}
