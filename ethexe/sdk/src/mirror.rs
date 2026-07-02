// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{VaraEthApi, types::InjectedMessageResult};
use alloy::rpc::types::TransactionReceipt;
use anyhow::{Context, Result, anyhow, bail, ensure};
use ethexe_common::{
    Address, BlockHeader, HashOf, SimpleBlockData,
    gear::ValueClaim,
    gear_core::rpc::ReplyInfo,
    injected::{
        InjectedTransaction, InjectedTransactionAcceptance, Promise, Receipt,
        SignedInjectedTransaction,
    },
};
use ethexe_ethereum::{
    IntoBlockId,
    mirror::{
        Mirror as EthereumMirror, MirrorEvents as EthereumMirrorEvents,
        MirrorQuery as EthereumMirrorQuery,
    },
};
use ethexe_rpc_client::{
    InjectedClient, ProgramClient,
    types::{CalculateReplyForHandleResult, FullProgramState, ProgramBestState},
};
use ethexe_runtime_common::state::{Mailbox, ProgramState, UserMailbox};
use futures::TryFutureExt;
use gprimitives::{ActorId, CodeId, H256, MessageId, U256};
use gsigner::secp256k1::Secp256k1SignerExt;
use jsonrpsee::core::client::Subscription;

pub struct Mirror<'a> {
    pub(crate) api: &'a VaraEthApi,
    pub(crate) mirror_client: EthereumMirror,
    pub(crate) mirror_query_client: EthereumMirrorQuery,
}

impl<'a> Mirror<'a> {
    pub fn events(&self) -> EthereumMirrorEvents<'_> {
        self.mirror_query_client.events()
    }

    pub fn actor_id(&self) -> ActorId {
        self.mirror_client.actor_id()
    }

    pub async fn balance(&self) -> Result<u128> {
        self.mirror_query_client.balance().await
    }

    pub async fn code_id(&self) -> Result<CodeId> {
        let code_id = self
            .api
            .vara_eth_client()
            .code_id(self.actor_id().to_address_lossy())
            .await?;
        Ok(code_id.into())
    }

    pub async fn wait_for_state_change(&self) -> Result<H256> {
        self.mirror_client.wait_for_state_change().await
    }

    pub async fn router(&self) -> Result<Address> {
        self.mirror_query_client.router().await
    }

    pub async fn state(&self) -> Result<(H256, ProgramState)> {
        let state_hash = self.state_hash().await?;
        let state = self.state_at_hash(state_hash).await?;
        Ok((state_hash, state))
    }

    pub async fn state_at_hash(&self, state_hash: H256) -> Result<ProgramState> {
        self.api
            .vara_eth_client()
            .read_state(state_hash)
            .await
            .map_err(Into::into)
    }

    pub async fn full_state(&self) -> Result<FullProgramState> {
        let state_hash = self.state_hash().await?;
        self.api
            .vara_eth_client()
            .read_full_state(state_hash)
            .map_err(Into::into)
            .await
    }

    pub async fn mailbox(&self, mailbox_hash: HashOf<Mailbox>) -> Result<Mailbox> {
        self.api
            .vara_eth_client
            .read_mailbox(mailbox_hash.inner())
            .map_err(Into::into)
            .await
    }

    pub async fn user_mailbox(
        &self,
        user_mailbox_hash: HashOf<UserMailbox>,
    ) -> Result<UserMailbox> {
        self.api
            .vara_eth_client
            .read_user_mailbox(user_mailbox_hash.inner())
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

    pub async fn inheritor(&self) -> Result<ActorId> {
        self.mirror_query_client.inheritor().await
    }

    pub async fn initializer(&self) -> Result<ActorId> {
        self.mirror_query_client.initializer().await
    }

    pub async fn calculate_reply_for_handle(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<CalculateReplyForHandleResult> {
        self.calculate_reply_for_handle_at(payload, value, None)
            .await
    }

    pub async fn calculate_reply_for_handle_with_top_up(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
        top_up: u128,
    ) -> Result<CalculateReplyForHandleResult> {
        self.calculate_reply_for_handle_at_with_top_up(payload, value, None, Some(top_up))
            .await
    }

    pub async fn calculate_reply_for_handle_at(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
        at: Option<H256>,
    ) -> Result<CalculateReplyForHandleResult> {
        self.calculate_reply_for_handle_at_with_top_up(payload, value, at, None)
            .await
    }

    pub async fn calculate_reply_for_handle_at_with_top_up(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
        at: Option<H256>,
        top_up: Option<u128>,
    ) -> Result<CalculateReplyForHandleResult> {
        let sender_address = self.api.ethereum_client.sender_address();
        let source: ActorId = sender_address.into();
        let destination = self.actor_id();
        self.api
            .vara_eth_client()
            .calculate_reply_for_handle(
                at,
                source.to_address_lossy(),
                destination.to_address_lossy(),
                payload.as_ref().to_vec().into(),
                value,
                top_up,
            )
            .map_err(Into::into)
            .await
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

    async fn prepare_injected_transaction_with_reference(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(SignedInjectedTransaction, u32, H256)> {
        // TODO: check existence of deposit in Router contract
        ensure!(
            value == 0,
            "injected transactions with non zero value are not supported for now"
        );

        let signer = self.api.ethereum_client.signer();
        let sender_address = self.api.ethereum_client.sender_address();
        let public_key = signer
            .get_key_by_address(sender_address)
            .with_context(|| "failed to get key for sender address")?
            .ok_or_else(|| anyhow!("no key found for sender address"))?;

        let destination = self.mirror_client.actor_id();
        let payload = payload
            .as_ref()
            .to_vec()
            .try_into()
            .context("payload is too large")?;

        let SimpleBlockData {
            hash: reference_block,
            header:
                BlockHeader {
                    height: reference_block_number,
                    ..
                },
        } = self.api.ethereum_client.get_latest_block().await?;
        let salt = H256::random()
            .0
            .to_vec()
            .try_into()
            .context("salt is too large")?;

        let injected_transaction = InjectedTransaction {
            destination,
            payload,
            value,
            reference_block,
            salt,
        };

        signer
            .signed_message(public_key, injected_transaction, None)
            .with_context(|| "failed to create signed injected transaction")
            .map(|transaction| (transaction, reference_block_number, reference_block))
    }

    pub async fn send_message_injected(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<MessageId> {
        self.send_message_injected_with_details(payload, value)
            .await
            .map(|result| result.message_id)
    }

    pub async fn send_message_injected_and_watch(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(MessageId, Promise)> {
        let result = self
            .send_message_injected_with_details_and_watch(payload, value)
            .await?;
        let promise = result
            .promise
            .expect("invariant: watch result always contains a promise");
        Ok((result.message_id, promise))
    }

    pub async fn send_message_injected_with_details(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<InjectedMessageResult> {
        let (transaction, reference_block_number, reference_block_hash) = self
            .prepare_injected_transaction_with_reference(payload, value)
            .await?;
        let injected_transaction = transaction.data();

        let message_id = injected_transaction.to_message_id();
        let tx_hash = injected_transaction.to_hash().into();

        let result: InjectedTransactionAcceptance = self
            .api
            .vara_eth_client()
            .send_transaction(transaction)
            .await
            .with_context(|| "failed to send injected transaction")?;

        match result {
            InjectedTransactionAcceptance::Accept => Ok(InjectedMessageResult {
                message_id,
                tx_hash,
                reference_block_number,
                reference_block_hash,
                promise: None,
            }),
            InjectedTransactionAcceptance::Reject { reason } => {
                Err(anyhow!("injected transaction was rejected: {reason}"))
            }
        }
    }

    pub async fn send_message_injected_with_details_and_watch(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<InjectedMessageResult> {
        let (transaction, reference_block_number, reference_block_hash) = self
            .prepare_injected_transaction_with_reference(payload, value)
            .await?;
        let injected_transaction = transaction.data();

        let message_id = injected_transaction.to_message_id();
        let tx_hash = injected_transaction.to_hash().into();

        let mut subscription = self
            .api
            .vara_eth_client()
            .send_transaction_and_watch(transaction)
            .await
            .with_context(|| "failed to send injected transaction and subscribe to it's promise")?;

        let receipt = subscription
            .next()
            .await
            .ok_or_else(|| anyhow!("no promise received from subscription"))?
            .with_context(|| "failed to receive transaction promise")?
            .data()
            .clone();
        let promise = match receipt {
            Receipt::Promise(promise) => promise,
            Receipt::Purged(err) => {
                bail!("injected transaction was purged: {err}")
            }
        };

        Ok(InjectedMessageResult {
            message_id,
            tx_hash,
            reference_block_number,
            reference_block_hash,
            promise: Some(promise),
        })
    }

    /// Subscribes to this program's best state, yielding a [`ProgramBestState`]
    /// on every newly computed MB that produces a transition for the program.
    pub async fn subscribe_best_state(&self) -> Result<Subscription<ProgramBestState>> {
        let program_id = self.actor_id().to_address_lossy();
        self.api
            .vara_eth_client
            .subscribe_best_state(program_id)
            .await
            .with_context(|| "failed to subscribe to program best state")
    }

    pub async fn wait_for_reply(&self, message_id: MessageId) -> Result<ReplyInfo> {
        self.mirror_client.wait_for_reply(message_id).await
    }

    pub async fn send_reply(
        &self,
        replied_to: MessageId,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(H256, MessageId)> {
        self.mirror_client
            .send_reply(replied_to, payload, value)
            .await
    }

    pub async fn send_reply_with_receipt(
        &self,
        replied_to: MessageId,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(TransactionReceipt, MessageId)> {
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

    pub async fn wait_for_value_claim(&self, message_id: MessageId) -> Result<ValueClaim> {
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

    pub async fn executable_balance_top_up_with_permit(&self, value: u128) -> Result<H256> {
        self.mirror_client
            .executable_balance_top_up_with_permit(value)
            .await
    }

    pub async fn executable_balance_top_up_with_permit_and_receipt(
        &self,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self.mirror_client
            .executable_balance_top_up_with_permit_and_receipt(value)
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
