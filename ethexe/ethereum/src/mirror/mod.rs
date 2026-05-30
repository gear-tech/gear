// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    AlloyProvider, Eip712PermitData, Ethereum, IntoBlockId, Sender, TryGetReceipt, WVara,
    abi::{self, IMirror},
    mirror::events::AllEventsBuilder,
};
use alloy::{
    contract::CallBuilder,
    eips::BlockId,
    network,
    primitives::{Address as AlloyAddress, Bytes, U256 as AlloyU256},
    providers::{PendingTransactionBuilder, Provider, RootProvider, WalletProvider},
    rpc::types::TransactionReceipt,
};
use anyhow::{Result, anyhow};
use ethexe_common::{
    Address,
    events::mirror::{ReplyEvent, StateChangedEvent, ValueClaimedEvent},
};
pub use events::signatures;
use events::{
    ExecutableBalanceTopUpRequestedEventBuilder, MessageCallFailedEventBuilder,
    MessageEventBuilder, MessageQueueingRequestedEventBuilder,
    OwnedBalanceTopUpRequestedEventBuilder, ReplyCallFailedEventBuilder, ReplyEventBuilder,
    ReplyQueueingRequestedEventBuilder, ReplyTransferFailedEventBuilder, StateChangedEventBuilder,
    TransferLockedValueToInheritorFailedEventBuilder, ValueClaimFailedEventBuilder,
    ValueClaimedEventBuilder, ValueClaimingRequestedEventBuilder,
};
use futures::StreamExt;
use gear_core::{ids::prelude::MessageIdExt, rpc::ReplyInfo};
use gprimitives::{ActorId, H256, MessageId, U256};
use serde::Serialize;

pub mod events;

/// Information about a successfully claimed value from a Mirror program.
#[derive(Debug, Clone, Serialize)]
pub struct ClaimInfo {
    /// Identifier of the message whose locked value was claimed.
    pub message_id: MessageId,
    /// Account that received the claimed value.
    pub actor_id: ActorId,
    /// Amount of value transferred, in the smallest denomination.
    pub value: u128,
}

type Instance = IMirror::IMirrorInstance<AlloyProvider>;
type QueryInstance = IMirror::IMirrorInstance<RootProvider>;

/// Write-capable handle to a deployed Mirror contract (one per Gear program on Ethereum).
///
/// Wraps an Alloy provider with a wallet signer so callers can send transactions
/// (`send_message`, `send_reply`, `claim_value`, balance top-ups) and subscribe to
/// real-time events via [`Mirror::query`].
pub struct Mirror {
    instance: Instance,
    wvara_address: AlloyAddress,
    sender: Sender,
}

impl Mirror {
    pub(crate) fn new(
        address: AlloyAddress,
        wvara_address: AlloyAddress,
        sender: Sender,
        provider: AlloyProvider,
    ) -> Self {
        Self {
            instance: Instance::new(address, provider),
            wvara_address,
            sender,
        }
    }

    /// Returns the Gear [`ActorId`] that corresponds to this Mirror's Ethereum address.
    pub fn actor_id(&self) -> ActorId {
        let address = Address(*self.instance.address().0);
        address.into()
    }

    /// Returns a read-only [`MirrorQuery`] handle bound to the same contract address.
    pub fn query(&self) -> MirrorQuery {
        MirrorQuery(QueryInstance::new(
            *self.instance.address(),
            self.instance.provider().root().clone(),
        ))
    }

    /// Returns a [`WVara`] handle for the Wrapped Vara token contract associated with this Mirror.
    pub fn wvara(&self) -> WVara {
        WVara::new(self.wvara_address, self.instance.provider().clone())
    }

    /// Subscribes to `StateChanged` events and returns the first new state hash observed.
    pub async fn wait_for_state_change(&self) -> Result<H256> {
        let mut stream = self.query().events().state_changed().subscribe().await?;

        while let Some(result) = stream.next().await {
            if let Ok((StateChangedEvent { state_hash }, _)) = result {
                return Ok(state_hash);
            }
        }

        Err(anyhow!("Failed to define if state changed"))
    }

    /// Sends a message to the program and returns `(transaction_hash, message_id)` on success.
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

    /// Sends a message and returns the full `TransactionReceipt` together with the `MessageId`.
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

    // TODO: remove gsobol's code that exposes alloy internals
    /// Submits a `sendMessage` call and returns a pending transaction builder without waiting for confirmation.
    pub async fn send_message_pending(
        &self,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<PendingTransactionBuilder<network::Ethereum>> {
        let call_reply = false;
        self.instance
            .sendMessage(payload.as_ref().to_vec().into(), call_reply)
            .value(AlloyU256::from(value))
            .send()
            .await
            .map_err(Into::into)
    }

    /// Subscribes to `Reply` events and returns the first reply addressed to `message_id`.
    pub async fn wait_for_reply(&self, message_id: MessageId) -> Result<ReplyInfo> {
        let mut stream = self.query().events().reply().subscribe().await?;

        while let Some(result) = stream.next().await {
            if let Ok((
                ReplyEvent {
                    payload,
                    value,
                    reply_to,
                    reply_code,
                },
                _,
            )) = result
                && reply_to == message_id
            {
                return Ok(ReplyInfo {
                    payload,
                    value,
                    code: reply_code,
                });
            }
        }

        Err(anyhow!("Failed to wait for reply"))
    }

    /// Sends a reply to `replied_to` and returns `(transaction_hash, message_id)` on success.
    pub async fn send_reply(
        &self,
        replied_to: MessageId,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(H256, MessageId)> {
        self.send_reply_with_receipt(replied_to, payload, value)
            .await
            .map(|(receipt, message_id)| ((*receipt.transaction_hash).into(), message_id))
    }

    /// Sends a reply to `replied_to` and returns the full `TransactionReceipt` together with the derived `MessageId`.
    pub async fn send_reply_with_receipt(
        &self,
        replied_to: MessageId,
        payload: impl AsRef<[u8]>,
        value: u128,
    ) -> Result<(TransactionReceipt, MessageId)> {
        let builder = self
            .instance
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
        let message_id = MessageId::generate_reply(replied_to);
        Ok((receipt, message_id))
    }

    /// Claims the value locked in message `claimed_id` and returns the transaction hash.
    pub async fn claim_value(&self, claimed_id: MessageId) -> Result<H256> {
        self.claim_value_with_receipt(claimed_id)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Claims the value locked in message `claimed_id` and returns the full `TransactionReceipt`.
    pub async fn claim_value_with_receipt(
        &self,
        claimed_id: MessageId,
    ) -> Result<TransactionReceipt> {
        let builder = self.instance.claimValue(claimed_id.into_bytes().into());
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        Ok(receipt)
    }

    /// Subscribes to `ValueClaimed` events and returns [`ClaimInfo`] once the event matching `message_id` is seen.
    pub async fn wait_for_value_claim(&self, message_id: MessageId) -> Result<ClaimInfo> {
        let mut stream = self.query().events().value_claimed().subscribe().await?;

        while let Some(result) = stream.next().await {
            if let Ok((ValueClaimedEvent { claimed_id, value }, _)) = result
                && claimed_id == message_id
            {
                let actor_id =
                    Address::from(self.instance.provider().default_signer_address()).into();
                return Ok(ClaimInfo {
                    message_id: claimed_id,
                    actor_id,
                    value,
                });
            }
        }

        Err(anyhow!("Failed to wait for value claimed"))
    }

    /// Tops up the program's executable balance by `value` and returns the transaction hash.
    pub async fn executable_balance_top_up(&self, value: u128) -> Result<H256> {
        self.executable_balance_top_up_with_receipt(value)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Tops up the program's executable balance by `value` and returns the full `TransactionReceipt`.
    pub async fn executable_balance_top_up_with_receipt(
        &self,
        value: u128,
    ) -> Result<TransactionReceipt> {
        let builder = self.instance.executableBalanceTopUp(value);
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        Ok(receipt)
    }

    /// Tops up the executable balance using an EIP-712 permit (no prior approval transaction needed) and returns the transaction hash.
    pub async fn executable_balance_top_up_with_permit(&self, value: u128) -> Result<H256> {
        self.executable_balance_top_up_with_permit_and_receipt(value)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Tops up the executable balance using an EIP-712 permit and returns the full `TransactionReceipt`.
    pub async fn executable_balance_top_up_with_permit_and_receipt(
        &self,
        value: u128,
    ) -> Result<TransactionReceipt> {
        let Eip712PermitData { deadline, v, r, s } = Ethereum::prepare_permit_data(
            self.instance.provider(),
            self.wvara().query(),
            &self.sender,
            self.actor_id(),
            value,
        )
        .await?;

        let builder = self
            .instance
            .executableBalanceTopUpWithPermit(value, deadline, v, r, s);
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        Ok(receipt)
    }

    /// Transfers any value locked in this Mirror to its inheritor program and returns the transaction hash.
    pub async fn transfer_locked_value_to_inheritor(&self) -> Result<H256> {
        self.transfer_locked_value_to_inheritor_with_receipt()
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Transfers any value locked in this Mirror to its inheritor program and returns the full `TransactionReceipt`.
    pub async fn transfer_locked_value_to_inheritor_with_receipt(
        &self,
    ) -> Result<TransactionReceipt> {
        let builder = self.instance.transferLockedValueToInheritor();
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        Ok(receipt)
    }

    /// Sends a plain ETH transfer of `value` to the Mirror contract to top up its owned balance, returning the transaction hash.
    pub async fn owned_balance_top_up(&self, value: u128) -> Result<H256> {
        self.owned_balance_top_up_with_receipt(value)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Sends a plain ETH transfer of `value` to the Mirror contract to top up its owned balance, returning the full `TransactionReceipt`.
    pub async fn owned_balance_top_up_with_receipt(
        &self,
        value: u128,
    ) -> Result<TransactionReceipt> {
        let builder = CallBuilder::new_raw(self.instance.provider(), Bytes::new())
            .to(*self.instance.address())
            .value(AlloyU256::from(value));
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        Ok(receipt)
    }
}

/// Read-only handle for querying Mirror contract state and subscribing to its events.
///
/// Obtained via [`Mirror::query`] or constructed directly with [`MirrorQuery::new`].
/// Uses an unauthenticated [`RootProvider`] so no wallet is required.
pub struct MirrorQuery(QueryInstance);

impl MirrorQuery {
    /// Creates a new `MirrorQuery` bound to `mirror_address` using the given provider.
    pub fn new(provider: RootProvider, mirror_address: Address) -> Self {
        Self(QueryInstance::new(
            AlloyAddress::new(mirror_address.0),
            provider,
        ))
    }

    /// Returns a [`MirrorEvents`] builder for subscribing to or filtering Mirror contract events.
    pub fn events(&self) -> MirrorEvents<'_> {
        MirrorEvents { query: self }
    }

    /// Returns the native ETH balance of the Mirror contract address.
    pub async fn balance(&self) -> Result<u128> {
        self.0
            .provider()
            .get_balance(*self.0.address())
            .await
            .map(abi::utils::uint256_to_u128_lossy)
            .map_err(Into::into)
    }

    /// Returns the address of the Router contract that governs this Mirror.
    pub async fn router(&self) -> Result<Address> {
        self.0
            .router()
            .call()
            .await
            .map(|res| Address(res.into()))
            .map_err(Into::into)
    }

    /// Returns the latest committed state hash of the program from the Mirror contract.
    pub async fn state_hash(&self) -> Result<H256> {
        self.state_hash_at(BlockId::latest()).await
    }

    /// Returns the program's state hash as it was at the specified block.
    pub async fn state_hash_at(&self, id: impl IntoBlockId) -> Result<H256> {
        self.0
            .stateHash()
            .block(id.into_block_id())
            .call()
            .await
            .map(|res| H256(res.0))
            .map_err(Into::into)
    }

    /// Returns the current nonce of the Mirror contract, used to sequence state transitions.
    pub async fn nonce(&self) -> Result<U256> {
        self.0
            .nonce()
            .call()
            .await
            .map(abi::utils::uint256_to_u256)
            .map_err(Into::into)
    }

    /// Returns `true` if the underlying Gear program has called `gr_exit` and the Mirror is in the exited state.
    pub async fn exited(&self) -> Result<bool> {
        self.0.exited().call().await.map_err(Into::into)
    }

    /// Returns the actor that inherits this program's locked value after it exits or terminates.
    pub async fn inheritor(&self) -> Result<ActorId> {
        self.0
            .inheritor()
            .call()
            .await
            .map(|res| Address(res.into()).into())
            .map_err(Into::into)
    }

    /// Returns the actor that originally initialized this program via `upload_program`.
    pub async fn initializer(&self) -> Result<ActorId> {
        self.0
            .initializer()
            .call()
            .await
            .map(|res| Address(res.into()).into())
            .map_err(Into::into)
    }
}

/// Factory for per-event-type subscription builders scoped to a single Mirror contract.
///
/// Obtained via [`MirrorQuery::events`]. Each method returns a typed builder that can
/// be subscribed to as a live stream or used to fetch historical logs.
pub struct MirrorEvents<'a> {
    query: &'a MirrorQuery,
}

impl<'a> MirrorEvents<'a> {
    /// Returns a builder that subscribes to all event types emitted by the Mirror contract.
    pub fn all(&self) -> AllEventsBuilder<'a> {
        AllEventsBuilder::new(self.query)
    }

    /// Returns a builder for `StateChanged` events (emitted when the program state hash updates).
    pub fn state_changed(&self) -> StateChangedEventBuilder<'a> {
        StateChangedEventBuilder::new(self.query)
    }

    /// Returns a builder for `MessageQueueingRequested` events (emitted when a user queues a message to the program).
    pub fn message_queueing_requested(&self) -> MessageQueueingRequestedEventBuilder<'a> {
        MessageQueueingRequestedEventBuilder::new(self.query)
    }

    /// Returns a builder for `ReplyQueueingRequested` events (emitted when a user queues a reply).
    pub fn reply_queueing_requested(&self) -> ReplyQueueingRequestedEventBuilder<'a> {
        ReplyQueueingRequestedEventBuilder::new(self.query)
    }

    /// Returns a builder for `ValueClaimingRequested` events (emitted when a user requests to claim locked value).
    pub fn value_claiming_requested(&self) -> ValueClaimingRequestedEventBuilder<'a> {
        ValueClaimingRequestedEventBuilder::new(self.query)
    }

    /// Returns a builder for `OwnedBalanceTopUpRequested` events (emitted when the owned balance is topped up via plain ETH transfer).
    pub fn owned_balance_top_up_requested(&self) -> OwnedBalanceTopUpRequestedEventBuilder<'a> {
        OwnedBalanceTopUpRequestedEventBuilder::new(self.query)
    }

    /// Returns a builder for `ExecutableBalanceTopUpRequested` events (emitted when the executable balance is topped up).
    pub fn executable_balance_top_up_requested(
        &self,
    ) -> ExecutableBalanceTopUpRequestedEventBuilder<'a> {
        ExecutableBalanceTopUpRequestedEventBuilder::new(self.query)
    }

    /// Returns a builder for `Message` events (emitted when the program sends an outgoing message).
    pub fn message(&self) -> MessageEventBuilder<'a> {
        MessageEventBuilder::new(self.query)
    }

    /// Returns a builder for `MessageCallFailed` events (emitted when an outgoing message call fails).
    pub fn message_call_failed(&self) -> MessageCallFailedEventBuilder<'a> {
        MessageCallFailedEventBuilder::new(self.query)
    }

    /// Returns a builder for `Reply` events (emitted when the program sends a reply to a queued message).
    pub fn reply(&self) -> ReplyEventBuilder<'a> {
        ReplyEventBuilder::new(self.query)
    }

    /// Returns a builder for `ReplyCallFailed` events (emitted when a reply call fails on-chain).
    pub fn reply_call_failed(&self) -> ReplyCallFailedEventBuilder<'a> {
        ReplyCallFailedEventBuilder::new(self.query)
    }

    /// Returns a builder for `ValueClaimed` events (emitted when locked value is successfully claimed).
    pub fn value_claimed(&self) -> ValueClaimedEventBuilder<'a> {
        ValueClaimedEventBuilder::new(self.query)
    }

    /// Returns a builder for `TransferLockedValueToInheritorFailed` events (emitted when inheritor value transfer fails).
    pub fn transfer_locked_value_to_inheritor_failed(
        &self,
    ) -> TransferLockedValueToInheritorFailedEventBuilder<'a> {
        TransferLockedValueToInheritorFailedEventBuilder::new(self.query)
    }

    /// Returns a builder for `ReplyTransferFailed` events (emitted when value transfer as part of a reply fails).
    pub fn reply_transfer_failed(&self) -> ReplyTransferFailedEventBuilder<'a> {
        ReplyTransferFailedEventBuilder::new(self.query)
    }

    /// Returns a builder for `ValueClaimFailed` events (emitted when a value claim attempt fails).
    pub fn value_claim_failed(&self) -> ValueClaimFailedEventBuilder<'a> {
        ValueClaimFailedEventBuilder::new(self.query)
    }
}
