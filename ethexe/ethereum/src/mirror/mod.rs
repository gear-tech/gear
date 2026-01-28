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
    AlloyProvider, IntoBlockId, TryGetReceipt,
    abi::{self, IMirror},
};
use alloy::{
    contract::CallBuilder,
    eips::BlockId,
    hex, network,
    primitives::{Address, Bytes, U256 as AlloyU256},
    providers::{PendingTransactionBuilder, Provider, RootProvider, WalletProvider},
    rpc::types::{Filter, Topic, TransactionReceipt},
};
use anyhow::{Context, Result, anyhow};
use ethexe_common::{Address as LocalAddress, events::MirrorEvent};
pub use events::signatures;
use futures::StreamExt;
use gear_core::message::ReplyCode;
use gprimitives::{ActorId, H256, MessageId, U256};
use serde::Serialize;

pub mod events;

#[derive(Debug, Clone, Serialize)]
pub struct ReplyInfo {
    pub message_id: MessageId,
    pub actor_id: ActorId,
    #[serde(with = "hex::serde")]
    pub payload: Vec<u8>,
    pub code: ReplyCode,
    pub value: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaimInfo {
    pub message_id: MessageId,
    pub actor_id: ActorId,
    pub value: u128,
}

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

    pub async fn wait_for_state_changed(&self) -> Result<()> {
        let filter = Filter::new()
            .address(*self.0.address())
            .event_signature(Topic::from_iter([signatures::STATE_CHANGED]));
        let mut mirror_events = self
            .0
            .provider()
            .subscribe_logs(&filter)
            .await?
            .into_stream();

        while let Some(log) = mirror_events.next().await {
            if let Some(signatures::STATE_CHANGED) = log.topic0().cloned() {
                return Ok(());
            }
        }

        Err(anyhow!("Failed to define if state changed"))
    }

    pub async fn get_reference_block(&self) -> Result<(u64, H256)> {
        let block_resp = self
            .0
            .provider()
            .get_block(BlockId::latest())
            .await
            .with_context(|| "failed to get latest block")?
            .ok_or_else(|| anyhow!("latest block not found"))?;
        let block_number = block_resp.number();
        let block_hash = block_resp.hash().0.into();
        Ok((block_number, block_hash))
    }

    pub async fn get_balance(&self) -> Result<u128> {
        self.0
            .provider()
            .get_balance(*self.0.address())
            .await
            .map(abi::utils::uint256_to_u128_lossy)
            .map_err(Into::into)
    }

    pub async fn owned_balance_top_up_with_receipt(
        &self,
        value: u128,
    ) -> Result<TransactionReceipt> {
        let builder = CallBuilder::new_raw(self.0.provider(), Bytes::new())
            .to(*self.0.address())
            .value(AlloyU256::from(value));
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;

        Ok(receipt)
    }

    pub async fn owned_balance_top_up(&self, value: u128) -> Result<H256> {
        let receipt = self.owned_balance_top_up_with_receipt(value).await?;
        Ok((*receipt.transaction_hash).into())
    }

    pub async fn executable_balance_top_up_with_receipt(
        &self,
        value: u128,
    ) -> Result<TransactionReceipt> {
        let builder = self.0.executableBalanceTopUp(value);
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;

        Ok(receipt)
    }

    pub async fn executable_balance_top_up(&self, value: u128) -> Result<H256> {
        let receipt = self.executable_balance_top_up_with_receipt(value).await?;
        Ok((*receipt.transaction_hash).into())
    }

    pub async fn send_message(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(H256, MessageId)> {
        self.send_message_pending(payload, value)
            .await?
            .try_get_message_send_receipt()
            .await
    }

    pub async fn send_message_with_receipt(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(TransactionReceipt, MessageId)> {
        let receipt = self
            .send_message_pending(payload, value)
            .await?
            .try_get_receipt_check_reverted()
            .await?;
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

        Ok((receipt, message_id))
    }

    pub async fn send_message_pending(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<PendingTransactionBuilder<network::Ethereum>> {
        let call_reply = false;
        self.0
            .sendMessage(payload.as_ref().to_vec().into(), call_reply)
            .value(AlloyU256::from(value))
            .send()
            .await
            .map_err(Into::into)
    }

    pub async fn wait_for_reply(&self, message_id: MessageId) -> Result<ReplyInfo> {
        let filter = Filter::new()
            .address(*self.0.address())
            .event_signature(Topic::from_iter([signatures::REPLY]));
        let mut mirror_events = self
            .0
            .provider()
            .subscribe_logs(&filter)
            .await?
            .into_stream();

        while let Some(log) = mirror_events.next().await {
            if let Some(signatures::REPLY) = log.topic0().cloned()
                && let MirrorEvent::Reply {
                    payload,
                    value,
                    reply_to,
                    reply_code,
                } = MirrorEvent::from(crate::decode_log::<IMirror::Reply>(&log)?)
                && reply_to == message_id
            {
                let actor_id = ActorId::from(*self.0.address());

                return Ok(ReplyInfo {
                    message_id: reply_to,
                    actor_id,
                    payload,
                    code: reply_code,
                    value,
                });
            }
        }

        Err(anyhow!("Failed to wait for reply"))
    }

    pub async fn wait_for_value_claimed(&self, message_id: MessageId) -> Result<ClaimInfo> {
        let filter = Filter::new()
            .address(*self.0.address())
            .event_signature(Topic::from_iter([signatures::VALUE_CLAIMED]));
        let mut mirror_events = self
            .0
            .provider()
            .subscribe_logs(&filter)
            .await?
            .into_stream();

        while let Some(log) = mirror_events.next().await {
            if let Some(signatures::VALUE_CLAIMED) = log.topic0().cloned()
                && let MirrorEvent::ValueClaimed { claimed_id, value } =
                    MirrorEvent::from(crate::decode_log::<IMirror::ValueClaimed>(&log)?)
                && claimed_id == message_id
            {
                let actor_id = self.0.provider().default_signer_address().into();
                return Ok(ClaimInfo {
                    message_id: claimed_id,
                    actor_id,
                    value,
                });
            }
        }

        Err(anyhow!("Failed to wait for value claimed"))
    }

    pub async fn send_reply(
        &self,
        replied_to: MessageId,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<H256> {
        let (receipt, _) = self
            .send_reply_with_receipt(replied_to, payload, value)
            .await?;
        Ok((*receipt.transaction_hash).into())
    }

    pub async fn send_reply_with_receipt(
        &self,
        replied_to: MessageId,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(TransactionReceipt, MessageId)> {
        let builder = self
            .0
            .sendReply(
                replied_to.into_bytes().into(),
                payload.as_ref().to_vec().into(),
            )
            .value(AlloyU256::from(value));
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;

        Ok((receipt, replied_to))
    }

    pub async fn claim_value(&self, claimed_id: MessageId) -> Result<H256> {
        let (receipt, _) = self.claim_value_with_receipt(claimed_id).await?;
        Ok((*receipt.transaction_hash).into())
    }

    pub async fn claim_value_with_receipt(
        &self,
        claimed_id: MessageId,
    ) -> Result<(TransactionReceipt, MessageId)> {
        let builder = self.0.claimValue(claimed_id.into_bytes().into());
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;

        Ok((receipt, claimed_id))
    }

    pub async fn transfer_locked_value_to_inheritor_with_receipt(
        &self,
    ) -> Result<TransactionReceipt> {
        let builder = self.0.transferLockedValueToInheritor();
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;

        Ok(receipt)
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

    pub async fn state_hash_at(&self, id: impl IntoBlockId) -> Result<H256> {
        self.0
            .stateHash()
            .block(id.into_block_id())
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
            .map(abi::utils::uint256_to_u256)
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
