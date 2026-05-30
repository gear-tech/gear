// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::VaraEthApi;
use alloy::rpc::types::TransactionReceipt;
use anyhow::{Context, Result, anyhow, bail, ensure};
use ethexe_common::{
    Address, SimpleBlockData,
    gear_core::rpc::ReplyInfo,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance, Promise,
        Receipt,
    },
};
use ethexe_ethereum::{
    IntoBlockId,
    mirror::{
        ClaimInfo, Mirror as EthereumMirror, MirrorEvents as EthereumMirrorEvents,
        MirrorQuery as EthereumMirrorQuery,
    },
};
use ethexe_rpc::{CalculateReplyForHandleResult, FullProgramState, InjectedClient, ProgramClient};
use ethexe_runtime_common::state::ProgramState;
use futures::TryFutureExt;
use gprimitives::{ActorId, CodeId, H256, MessageId, U256};
use gsigner::secp256k1::Secp256k1SignerExt;

/// SDK wrapper for a single on-chain Gear program (Mirror contract).
///
/// Obtained from [`VaraEthApi::mirror`]; borrows the parent `VaraEthApi` and cannot outlive it.
/// Combines an Ethereum contract client for write operations with a query client for read-only
/// calls, and delegates state queries to the ethexe JSON-RPC endpoint.
pub struct Mirror<'a> {
    pub(crate) api: &'a VaraEthApi,
    pub(crate) mirror_client: EthereumMirror,
    pub(crate) mirror_query_client: EthereumMirrorQuery,
}

impl<'a> Mirror<'a> {
    /// Returns an event subscription handle for Mirror contract events.
    pub fn events(&self) -> EthereumMirrorEvents<'_> {
        self.mirror_query_client.events()
    }

    /// Returns the `ActorId` of this program as registered on-chain.
    pub fn actor_id(&self) -> ActorId {
        self.mirror_client.actor_id()
    }

    /// Returns the native ETH balance of the Mirror contract address.
    pub async fn balance(&self) -> Result<u128> {
        self.mirror_query_client.balance().await
    }

    /// Returns the `CodeId` of the WASM code deployed for this program.
    pub async fn code_id(&self) -> Result<CodeId> {
        let code_id = self
            .api
            .vara_eth_client
            .code_id(self.actor_id().to_address_lossy())
            .await?;
        Ok(code_id.into())
    }

    /// Waits until the Mirror contract emits a state-change event and returns the new state hash.
    pub async fn wait_for_state_change(&self) -> Result<H256> {
        self.mirror_client.wait_for_state_change().await
    }

    /// Returns the address of the Router contract this Mirror is registered with.
    pub async fn router(&self) -> Result<Address> {
        self.mirror_query_client.router().await
    }

    /// Fetches the current [`ProgramState`] by resolving the on-chain state hash via the RPC node.
    pub async fn state(&self) -> Result<ProgramState> {
        let state_hash = self.state_hash().await?;
        self.api
            .vara_eth_client
            .read_state(state_hash)
            .map_err(Into::into)
            .await
    }

    /// Fetches the full program state, including queue and storage details, via the RPC node.
    pub async fn full_state(&self) -> Result<FullProgramState> {
        let state_hash = self.state_hash().await?;
        self.api
            .vara_eth_client
            .read_full_state(state_hash)
            .map_err(Into::into)
            .await
    }

    /// Returns the current state hash stored in the Mirror contract.
    pub async fn state_hash(&self) -> Result<H256> {
        self.mirror_query_client.state_hash().await
    }

    /// Returns the state hash stored in the Mirror contract at the given block.
    pub async fn state_hash_at(&self, id: impl IntoBlockId) -> Result<H256> {
        self.mirror_query_client.state_hash_at(id).await
    }

    /// Returns the current message nonce of this program, used to derive outgoing `MessageId`s.
    pub async fn nonce(&self) -> Result<U256> {
        self.mirror_query_client.nonce().await
    }

    /// Returns `true` if the program has called `gr_exit` and is no longer executable.
    pub async fn exited(&self) -> Result<bool> {
        self.mirror_query_client.exited().await
    }

    /// Returns the `ActorId` designated to inherit this program's locked value after it exits.
    pub async fn inheritor(&self) -> Result<ActorId> {
        self.mirror_query_client.inheritor().await
    }

    /// Returns the `ActorId` that originally initialized this program.
    pub async fn initializer(&self) -> Result<ActorId> {
        self.mirror_query_client.initializer().await
    }

    /// Calculates the reply the program would produce for a handle message at the current block.
    ///
    /// Delegates to [`calculate_reply_for_handle_at`](Self::calculate_reply_for_handle_at) with
    /// `at = None`.
    pub async fn calculate_reply_for_handle(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<CalculateReplyForHandleResult> {
        self.calculate_reply_for_handle_at(payload, value, None)
            .await
    }

    /// Calculates the reply the program would produce for a handle message at a specific block hash.
    ///
    /// Passes the caller's Ethereum address as the message source. `at = None` means the latest
    /// committed state.
    pub async fn calculate_reply_for_handle_at(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
        at: Option<H256>,
    ) -> Result<CalculateReplyForHandleResult> {
        let sender_address = self.api.ethereum_client.sender_address();
        let source: ActorId = sender_address.into();
        let destination = self.actor_id();
        self.api
            .vara_eth_client
            .calculate_reply_for_handle(
                at,
                source.to_address_lossy(),
                destination.to_address_lossy(),
                payload.as_ref().to_vec().into(),
                value,
            )
            .map_err(Into::into)
            .await
    }

    /// Sends an on-chain handle message to this program and returns the transaction hash and the
    /// resulting `MessageId`.
    pub async fn send_message(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(H256, MessageId)> {
        self.mirror_client.send_message(payload, value).await
    }

    /// Sends an on-chain handle message and returns the full Ethereum `TransactionReceipt` together
    /// with the resulting `MessageId`.
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
            ..
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

        let transaction = AddressedInjectedTransaction {
            recipient: Address::default(),
            tx: signer
                .signed_message(public_key, injected_transaction, None)
                .with_context(|| "failed to create signed injected transaction")?,
        };

        Ok(transaction)
    }

    /// Submits an injected (off-chain signed) handle message via the ethexe RPC node.
    ///
    /// Only zero-value messages are currently supported. Returns the `MessageId` on `Accept` or
    /// `AlreadyPooled`; returns an error on `Reject`.
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
            InjectedTransactionAcceptance::Accept
            | InjectedTransactionAcceptance::AlreadyPooled { .. } => Ok(message_id),
            InjectedTransactionAcceptance::Reject { reason } => {
                Err(anyhow!("injected transaction was rejected: {reason}"))
            }
        }
    }

    /// Submits an injected handle message and waits for the node to return a [`Promise`] receipt.
    ///
    /// Only zero-value messages are currently supported. Returns the `MessageId` and the `Promise`
    /// produced by the node. Errors if the transaction is purged (`Receipt::Purged`) or the
    /// subscription stream closes without producing an event.
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

        Ok((message_id, promise))
    }

    /// Waits until the Mirror contract emits a reply event for the given `message_id` and returns
    /// the reply details.
    pub async fn wait_for_reply(&self, message_id: MessageId) -> Result<ReplyInfo> {
        self.mirror_client.wait_for_reply(message_id).await
    }

    /// Sends an on-chain reply to a previously received message and returns the transaction hash
    /// and the resulting `MessageId`.
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

    /// Sends an on-chain reply and returns the full `TransactionReceipt` together with the
    /// resulting `MessageId`.
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

    /// Claims the value locked in the mailbox for `claimed_id` and returns the transaction hash.
    pub async fn claim_value(&self, claimed_id: MessageId) -> Result<H256> {
        self.mirror_client.claim_value(claimed_id).await
    }

    /// Claims the value locked in the mailbox for `claimed_id` and returns the full
    /// `TransactionReceipt`.
    pub async fn claim_value_with_receipt(
        &self,
        claimed_id: MessageId,
    ) -> Result<TransactionReceipt> {
        self.mirror_client
            .claim_value_with_receipt(claimed_id)
            .await
    }

    /// Waits until the Mirror emits a value-claimed event for `message_id` and returns the claim
    /// details.
    pub async fn wait_for_value_claim(&self, message_id: MessageId) -> Result<ClaimInfo> {
        self.mirror_client.wait_for_value_claim(message_id).await
    }

    /// Tops up the executable balance of this program by `value` WVara tokens and returns the
    /// transaction hash.
    pub async fn executable_balance_top_up(&self, value: u128) -> Result<H256> {
        self.mirror_client.executable_balance_top_up(value).await
    }

    /// Tops up the executable balance of this program by `value` WVara tokens and returns the full
    /// `TransactionReceipt`.
    pub async fn executable_balance_top_up_with_receipt(
        &self,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self.mirror_client
            .executable_balance_top_up_with_receipt(value)
            .await
    }

    /// Tops up the executable balance using an ERC-20 permit signature (gasless approval) and
    /// returns the transaction hash.
    pub async fn executable_balance_top_up_with_permit(&self, value: u128) -> Result<H256> {
        self.mirror_client
            .executable_balance_top_up_with_permit(value)
            .await
    }

    /// Tops up the executable balance using an ERC-20 permit signature and returns the full
    /// `TransactionReceipt`.
    pub async fn executable_balance_top_up_with_permit_and_receipt(
        &self,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self.mirror_client
            .executable_balance_top_up_with_permit_and_receipt(value)
            .await
    }

    /// Transfers any value locked in this exited program to its designated inheritor and returns
    /// the transaction hash.
    pub async fn transfer_locked_value_to_inheritor(&self) -> Result<H256> {
        self.mirror_client
            .transfer_locked_value_to_inheritor()
            .await
    }

    /// Transfers locked value to the inheritor and returns the full `TransactionReceipt`.
    pub async fn transfer_locked_value_to_inheritor_with_receipt(
        &self,
    ) -> Result<TransactionReceipt> {
        self.mirror_client
            .transfer_locked_value_to_inheritor_with_receipt()
            .await
    }

    /// Tops up the owned (non-executable) balance of this program and returns the transaction hash.
    pub async fn owned_balance_top_up(&self, value: u128) -> Result<H256> {
        self.owned_balance_top_up_with_receipt(value)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Tops up the owned balance of this program and returns the full `TransactionReceipt`.
    pub async fn owned_balance_top_up_with_receipt(
        &self,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self.mirror_client
            .owned_balance_top_up_with_receipt(value)
            .await
    }
}
