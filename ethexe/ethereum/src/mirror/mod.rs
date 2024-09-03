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

use crate::{abi::IMirror, AlloyProvider, AlloyTransport};
use alloy::{
    primitives::Address,
    providers::{Provider, ProviderBuilder, RootProvider},
    transports::BoxTransport,
};
use anyhow::{anyhow, Result};
use ethexe_signer::Address as LocalAddress;
use events::signatures;
use gprimitives::{MessageId, H256};
use std::sync::Arc;

pub mod events;

type InstanceProvider = Arc<AlloyProvider>;
type Instance = IMirror::IMirrorInstance<AlloyTransport, InstanceProvider>;

type QueryInstance = IMirror::IMirrorInstance<AlloyTransport, Arc<RootProvider<BoxTransport>>>;

pub struct Mirror(Instance);

impl Mirror {
    pub(crate) fn new(address: Address, provider: InstanceProvider) -> Self {
        Self(Instance::new(address, provider))
    }

    pub fn address(&self) -> LocalAddress {
        LocalAddress(*self.0.address().0)
    }

    pub fn query(&self) -> MirrorQuery {
        MirrorQuery(QueryInstance::new(
            *self.0.address(),
            Arc::new(self.0.provider().root().clone()),
        ))
    }

    pub async fn send_message(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(H256, MessageId)> {
        let builder = self.0.sendMessage(payload.as_ref().to_vec().into(), value);
        let tx = builder.send().await?;

        let receipt = crate::get_transaction_receipt(tx).await?;

        let tx_hash = (*receipt.transaction_hash).into();
        let mut message_id = None;

        for log in receipt.inner.logs() {
            if log.topic0().map(|v| v.0)
                == Some(signatures::MESSAGE_QUEUEING_REQUESTED.to_fixed_bytes())
            {
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
        let tx = builder.send().await?;

        let receipt = crate::get_transaction_receipt(tx).await?;
        Ok((*receipt.transaction_hash).into())
    }

    pub async fn claim_value(&self, claimed_id: MessageId) -> Result<H256> {
        let builder = self.0.claimValue(claimed_id.into_bytes().into());
        let tx = builder.send().await?;

        let receipt = crate::get_transaction_receipt(tx).await?;

        Ok((*receipt.transaction_hash).into())
    }
}

pub struct MirrorQuery(QueryInstance);

impl MirrorQuery {
    pub async fn new(rpc_url: &str, router_address: LocalAddress) -> Result<Self> {
        let provider = Arc::new(ProviderBuilder::new().on_builtin(rpc_url).await?);

        Ok(Self(QueryInstance::new(
            Address::new(router_address.0),
            provider,
        )))
    }

    pub async fn state_hash(&self) -> Result<H256> {
        self.0
            .stateHash()
            .call()
            .await
            .map(|res| H256(*res._0))
            .map_err(Into::into)
    }
}
