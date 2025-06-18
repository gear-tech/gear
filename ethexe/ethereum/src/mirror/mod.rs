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

use crate::{abi::IMirror, AlloyProvider, TryGetReceipt};
use alloy::{
    eips::BlockId,
    primitives::{Address, U256},
    providers::{Provider, RootProvider},
};
use anyhow::{anyhow, Result};
use ethexe_common::Address as LocalAddress;
use events::signatures;
use gprimitives::{MessageId, H256};

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

    pub async fn executable_balance_top_up(&self, value: u128) -> Result<H256> {
        let builder = self.0.executableBalanceTopUp(value);
        let receipt = builder.send().await?.try_get_receipt().await?;

        Ok((*receipt.transaction_hash).into())
    }

    pub async fn send_message(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(H256, MessageId)> {
        let builder = self
            .0
            .sendMessage(payload.as_ref().to_vec().into(), value, false);
        let receipt = builder.send().await?.try_get_receipt().await?;

        let tx_hash = (*receipt.transaction_hash).into();
        let mut message_id = None;

        for log in receipt.inner.logs() {
            if log.topic0() == Some(&signatures::MESSAGE_QUEUEING_REQUESTED) {
                let event = crate::decode_log::<IMirror::MessageQueueingRequested>(log)?;

                message_id = Some((*event.id).into());

                break;
            }
        }

        let message_id =
            message_id.ok_or_else(|| anyhow!("Couldn't find `MessageQueueingRequested` log"))?;

        Ok((tx_hash, message_id))
    }

    pub async fn send_reply(
        &self,
        replied_to: MessageId,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<H256> {
        let builder = self.0.sendReply(
            replied_to.into_bytes().into(),
            payload.as_ref().to_vec().into(),
            value,
        );
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

    pub async fn state_hash_at(&self, block: H256) -> Result<H256> {
        self.0
            .stateHash()
            .block(BlockId::hash(block.0.into()))
            .call()
            .await
            .map(|res| H256(*res._0))
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

    pub async fn inheritor(&self) -> Result<LocalAddress> {
        self.0
            .inheritor()
            .call()
            .await
            .map(|res| LocalAddress(res.into()))
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

    pub async fn router(&self) -> Result<LocalAddress> {
        self.0
            .router()
            .call()
            .await
            .map(|res| LocalAddress(res.into()))
            .map_err(Into::into)
    }
}
