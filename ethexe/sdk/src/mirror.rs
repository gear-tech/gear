// This file is part of Gear.

// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::VaraEthApi;
use alloy::rpc::types::TransactionReceipt;
use anyhow::{Context, Result, anyhow, ensure};
use ethexe_common::{
    Address,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance, Promise,
    },
};
use ethexe_ethereum::{
    IntoBlockId,
    mirror::{
        ClaimInfo, Mirror as EthereumMirror, MirrorEvents as EthereumMirrorEvents,
        MirrorQuery as EthereumMirrorQuery, ReplyInfo,
    },
};
use ethexe_rpc::{FullProgramState, InjectedClient, ProgramClient};
use ethexe_runtime_common::state::ProgramState;
use futures::TryFutureExt;
use gprimitives::{CodeId, H256, MessageId, U256};

pub struct Mirror<'a> {
    pub(crate) api: &'a VaraEthApi,
    pub(crate) mirror_client: EthereumMirror,
    pub(crate) mirror_query_client: EthereumMirrorQuery,
}

impl<'a> Mirror<'a> {
    pub fn events(&self) -> EthereumMirrorEvents<'_> {
        self.mirror_query_client.events()
    }

    pub fn address(&self) -> Address {
        self.mirror_client.address()
    }

    pub async fn balance(&self) -> Result<u128> {
        self.mirror_query_client.balance().await
    }

    pub async fn code_id(&self) -> Result<CodeId> {
        let code_id = self
            .api
            .vara_eth_client
            .code_id(self.mirror_client.address().0.into())
            .await?;
        Ok(code_id.into())
    }

    pub async fn wait_for_state_change(&self) -> Result<H256> {
        self.mirror_client.wait_for_state_change().await
    }

    pub async fn router(&self) -> Result<Address> {
        self.mirror_query_client.router().await
    }

    pub async fn state(&self) -> Result<ProgramState> {
        let state_hash = self.state_hash().await?;
        self.api
            .vara_eth_client
            .read_state(state_hash)
            .map_err(Into::into)
            .await
    }

    pub async fn full_state(&self) -> Result<FullProgramState> {
        let state_hash = self.state_hash().await?;
        self.api
            .vara_eth_client
            .read_full_state(state_hash)
            .map_err(Into::into)
            .await
    }

    pub async fn state_hash(&self) -> Result<H256> {
        self.mirror_query_client.state_hash().await
    }

    pub async fn state_hash_at(&self, id: impl IntoBlockId) -> Result<H256> {
        self.mirror_query_client.state_hash_at(id).await
    }

    pub async fn nonce(&self) -> Result<U256> {
        self.mirror_query_client.nonce().await
    }

    pub async fn exited(&self) -> Result<bool> {
        self.mirror_query_client.exited().await
    }

    pub async fn inheritor(&self) -> Result<Address> {
        self.mirror_query_client.inheritor().await
    }

    pub async fn initializer(&self) -> Result<Address> {
        self.mirror_query_client.initializer().await
    }

    pub async fn send_message(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(H256, MessageId)> {
        self.mirror_client.send_message(payload, value).await
    }

    pub async fn send_message_with_receipt(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(TransactionReceipt, MessageId)> {
        self.mirror_client
            .send_message_with_receipt(payload, value)
            .await
    }

    async fn prepare_injected_transaction(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<AddressedInjectedTransaction> {
        ensure!(value == 0, "injected transactions must have zero value"); // FIXME

        let signer = self
            .api
            .ethereum_client
            .signer()
            .with_context(|| "no signer available for sending injected transaction")?;
        let sender_address = self
            .api
            .ethereum_client
            .sender_address()
            .with_context(|| "no sender address available for sending injected transaction")?;
        let public_key = signer
            .storage()
            .get_key_by_addr(sender_address)
            .with_context(|| "failed to get key for sender address")?
            .ok_or_else(|| anyhow!("no key found for sender address"))?;

        let destination = self.mirror_client.address().into();
        let payload = payload
            .as_ref()
            .try_into()
            .with_context(|| "payload too large")?;
        let (_, reference_block) = self.api.ethereum_client.get_latest_block().await?;
        let salt = U256::from(H256::random().0);

        let injected_transaction = InjectedTransaction {
            destination,
            payload,
            value,
            reference_block,
            salt,
        };

        let transaction = AddressedInjectedTransaction {
            recipient: Address::default(),
            tx: signer
                .signed_message(public_key, injected_transaction)
                .with_context(|| "failed to create signed injected transaction")?,
        };

        Ok(transaction)
    }

    pub async fn send_message_injected(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<MessageId> {
        let transaction = self.prepare_injected_transaction(payload, value).await?;
        let injected_transaction = transaction.tx.data();

        let message_id = injected_transaction.to_message_id();

        let result: InjectedTransactionAcceptance = self
            .api
            .vara_eth_client
            .send_transaction(transaction)
            .await
            .with_context(|| "failed to send injected transaction")?;

        match result {
            InjectedTransactionAcceptance::Accept => Ok(message_id),
            InjectedTransactionAcceptance::Reject { reason } => {
                Err(anyhow!("injected transaction was rejected: {reason}"))
            }
        }
    }

    pub async fn send_message_injected_and_watch(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(MessageId, Promise)> {
        let transaction = self.prepare_injected_transaction(payload, value).await?;
        let injected_transaction = transaction.tx.data();

        let message_id = injected_transaction.to_message_id();

        let mut subscription = self
            .api
            .vara_eth_client
            .send_transaction_and_watch(transaction)
            .await
            .with_context(|| "failed to send injected transaction and subscribe to it's promise")?;

        let promise = subscription
            .next()
            .await
            .ok_or_else(|| anyhow!("no promise received from subscription"))?
            .with_context(|| "failed to receive transaction promise")?
            .into_data();

        Ok((message_id, promise))
    }

    pub async fn wait_for_reply(&self, message_id: MessageId) -> Result<ReplyInfo> {
        self.mirror_client.wait_for_reply(message_id).await
    }

    pub async fn send_reply(
        &self,
        replied_to: MessageId,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<H256> {
        self.mirror_client
            .send_reply(replied_to, payload, value)
            .await
    }

    pub async fn send_reply_with_receipt(
        &self,
        replied_to: MessageId,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self.mirror_client
            .send_reply_with_receipt(replied_to, payload, value)
            .await
    }

    pub async fn claim_value(&self, claimed_id: MessageId) -> Result<H256> {
        self.mirror_client.claim_value(claimed_id).await
    }

    pub async fn claim_value_with_receipt(
        &self,
        claimed_id: MessageId,
    ) -> Result<TransactionReceipt> {
        self.mirror_client
            .claim_value_with_receipt(claimed_id)
            .await
    }

    pub async fn wait_for_value_claim(&self, message_id: MessageId) -> Result<ClaimInfo> {
        self.mirror_client.wait_for_value_claim(message_id).await
    }

    pub async fn executable_balance_top_up(&self, value: u128) -> Result<H256> {
        self.mirror_client.executable_balance_top_up(value).await
    }

    pub async fn executable_balance_top_up_with_receipt(
        &self,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self.mirror_client
            .executable_balance_top_up_with_receipt(value)
            .await
    }

    pub async fn transfer_locked_value_to_inheritor(&self) -> Result<H256> {
        self.mirror_client
            .transfer_locked_value_to_inheritor()
            .await
    }

    pub async fn transfer_locked_value_to_inheritor_with_receipt(
        &self,
    ) -> Result<TransactionReceipt> {
        self.mirror_client
            .transfer_locked_value_to_inheritor_with_receipt()
            .await
    }

    pub async fn owned_balance_top_up(&self, value: u128) -> Result<H256> {
        self.owned_balance_top_up_with_receipt(value)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    pub async fn owned_balance_top_up_with_receipt(
        &self,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self.mirror_client
            .owned_balance_top_up_with_receipt(value)
            .await
    }
}
