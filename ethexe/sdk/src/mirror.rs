// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{VaraEthApi, types::InjectedMessageResult};
use alloy::{eips::BlockId, rpc::types::TransactionReceipt};
use anyhow::{Context, Result, anyhow, bail, ensure};
use ethexe_common::{
    Address, BlockHeader, HashOf, MaybeHashOf, OutgoingAction, OutgoingActions, SimpleBlockData,
    events::mirror::StateChangedEvent,
    gear::ValueClaim,
    gear_core::{buffer::Payload, memory::PageBuf, pages::GearPage, rpc::ReplyInfo},
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
    types::{CalculateReplyForHandleResult, FullProgramState, ProgramBestState, Proof},
};
use ethexe_runtime_common::state::{
    ActiveProgram, DispatchStash, Mailbox, MemoryPages, MemoryPagesRegion, MessageQueue, Program,
    ProgramState, UserMailbox, Waitlist,
};
use futures::{StreamExt, TryFutureExt};
use gprimitives::{ActorId, CodeId, H256, MessageId, U256};
use gsigner::secp256k1::Secp256k1SignerExt;
use jsonrpsee::core::client::Subscription;
use parity_scale_codec::Decode;

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

    pub async fn state_at(&self, id: impl IntoBlockId) -> Result<ProgramState> {
        let state_hash = self.state_hash_at(id).await?;
        self.state_at_hash(state_hash).await
    }

    pub async fn full_state(&self) -> Result<FullProgramState> {
        let state_hash = self.state_hash().await?;
        self.api
            .vara_eth_client()
            .read_full_state(state_hash)
            .map_err(Into::into)
            .await
    }

    pub async fn full_state_at(&self, id: impl IntoBlockId) -> Result<FullProgramState> {
        let state_hash = self.state_hash_at(id).await?;
        self.api
            .vara_eth_client()
            .read_full_state(state_hash)
            .map_err(Into::into)
            .await
    }

    pub async fn queue(
        &self,
        queue_hash: MaybeHashOf<MessageQueue>,
    ) -> Result<Option<MessageQueue>> {
        match queue_hash.to_inner() {
            Some(queue_hash) => self.queue_unchecked(queue_hash).await.map(Some),
            None => Ok(None),
        }
    }

    pub async fn queue_unchecked(&self, queue_hash: HashOf<MessageQueue>) -> Result<MessageQueue> {
        self.api
            .vara_eth_client
            .read_queue(queue_hash.inner())
            .map_err(Into::into)
            .await
    }

    pub async fn waitlist(&self, waitlist_hash: MaybeHashOf<Waitlist>) -> Result<Option<Waitlist>> {
        match waitlist_hash.to_inner() {
            Some(waitlist_hash) => self.waitlist_unchecked(waitlist_hash).await.map(Some),
            None => Ok(None),
        }
    }

    pub async fn waitlist_unchecked(&self, waitlist_hash: HashOf<Waitlist>) -> Result<Waitlist> {
        self.api
            .vara_eth_client
            .read_waitlist(waitlist_hash.inner())
            .map_err(Into::into)
            .await
    }

    pub async fn stash(
        &self,
        stash_hash: MaybeHashOf<DispatchStash>,
    ) -> Result<Option<DispatchStash>> {
        match stash_hash.to_inner() {
            Some(stash_hash) => self.stash_unchecked(stash_hash).await.map(Some),
            None => Ok(None),
        }
    }

    pub async fn stash_unchecked(
        &self,
        stash_hash: HashOf<DispatchStash>,
    ) -> Result<DispatchStash> {
        self.api
            .vara_eth_client
            .read_stash(stash_hash.inner())
            .map_err(Into::into)
            .await
    }

    pub async fn mailbox(&self, mailbox_hash: MaybeHashOf<Mailbox>) -> Result<Option<Mailbox>> {
        match mailbox_hash.to_inner() {
            Some(mailbox_hash) => self.mailbox_unchecked(mailbox_hash).await.map(Some),
            None => Ok(None),
        }
    }

    pub async fn mailbox_unchecked(&self, mailbox_hash: HashOf<Mailbox>) -> Result<Mailbox> {
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

    pub async fn outgoing_actions(&self, state_hash: H256) -> Result<OutgoingActions> {
        self.api
            .vara_eth_client
            .read_outgoing_actions(state_hash)
            .map_err(Into::into)
            .await
    }

    pub async fn outgoing_action_merkle_proof(
        &self,
        state_hash: H256,
        message_id: MessageId,
    ) -> Result<Proof> {
        self.api
            .vara_eth_client
            .read_outgoing_action_merkle_proof(state_hash, message_id)
            .map_err(Into::into)
            .await
    }

    pub async fn memory(&self) -> Result<Option<MirrorMemory<'_, 'a>>> {
        self.memory_at(BlockId::latest()).await
    }

    pub async fn memory_at(&self, id: impl IntoBlockId) -> Result<Option<MirrorMemory<'_, 'a>>> {
        let ProgramState {
            program: Program::Active(ActiveProgram { pages_hash, .. }),
            ..
        } = self.state_at(id).await?
        else {
            return Ok(None);
        };

        Ok(Some(MirrorMemory {
            mirror: self,
            pages_hash,
        }))
    }

    pub async fn memory_pages(
        &self,
        pages_hash: MaybeHashOf<MemoryPages>,
    ) -> Result<Option<MemoryPages>> {
        match pages_hash.to_inner() {
            Some(pages_hash) => self.memory_pages_unchecked(pages_hash).await.map(Some),
            None => Ok(None),
        }
    }

    pub async fn memory_pages_unchecked(
        &self,
        pages_hash: HashOf<MemoryPages>,
    ) -> Result<MemoryPages> {
        self.api
            .vara_eth_client
            .read_pages(pages_hash.inner())
            .map_err(Into::into)
            .await
    }

    pub async fn memory_page_region(
        &self,
        region_hash: MaybeHashOf<MemoryPagesRegion>,
    ) -> Result<Option<MemoryPagesRegion>> {
        match region_hash.to_inner() {
            Some(region_hash) => self
                .memory_page_region_unchecked(region_hash)
                .await
                .map(Some),
            None => Ok(None),
        }
    }

    pub async fn memory_page_region_unchecked(
        &self,
        region_hash: HashOf<MemoryPagesRegion>,
    ) -> Result<MemoryPagesRegion> {
        self.api
            .vara_eth_client
            .read_page_region(region_hash.inner())
            .map_err(Into::into)
            .await
    }

    pub async fn page_data(&self, page_hash: HashOf<PageBuf>) -> Result<PageBuf> {
        let encoded = self
            .api
            .vara_eth_client
            .read_page_data(page_hash.inner())
            .await?;

        let mut encoded = encoded.as_ref();
        PageBuf::decode(&mut encoded)
            .with_context(|| "failed to decode page data returned by Vara.ETH RPC")
    }

    pub async fn payload(&self, payload_hash: HashOf<Payload>) -> Result<Payload> {
        let payload = self
            .api
            .vara_eth_client
            .read_payload(payload_hash.inner())
            .await?;

        Payload::try_from(payload.to_vec())
            .map_err(|_| anyhow!("payload returned by Vara.ETH RPC exceeds size limit"))
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
        self.wait_for_value_claim_with_receipt(message_id)
            .await
            .map(|(_, claim_info)| claim_info)
    }

    pub async fn wait_for_value_claim_with_receipt(
        &self,
        message_id: MessageId,
    ) -> Result<(TransactionReceipt, ValueClaim)> {
        let mut stream = self.events().state_changed().subscribe().await?;

        while let Some(result) = stream.next().await {
            if let Ok((StateChangedEvent { state_hash }, _)) = result
                && let Ok(Proof {
                    total_leaves,
                    leaf_index,
                    outgoing_action,
                    proof,
                }) = self
                    .outgoing_action_merkle_proof(state_hash, message_id)
                    .await
            {
                // TODO: in future it will be `if let`
                let OutgoingAction::ValueClaim(value_claim) = outgoing_action.clone();
                let receipt = self
                    .process_outgoing_action_with_receipt(
                        state_hash,
                        total_leaves,
                        leaf_index,
                        outgoing_action,
                        proof,
                    )
                    .await?;
                return Ok((receipt, value_claim));
            }
        }

        Err(anyhow!("Failed to wait for value claimed"))
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

    pub async fn process_outgoing_action(
        &self,
        state_hash: H256,
        total_leaves: U256,
        leaf_index: U256,
        outgoing_action: OutgoingAction,
        proof: Vec<H256>,
    ) -> Result<H256> {
        self.mirror_client
            .process_outgoing_action(state_hash, total_leaves, leaf_index, outgoing_action, proof)
            .await
    }

    pub async fn process_outgoing_action_with_receipt(
        &self,
        state_hash: H256,
        total_leaves: U256,
        leaf_index: U256,
        outgoing_action: OutgoingAction,
        proof: Vec<H256>,
    ) -> Result<TransactionReceipt> {
        self.mirror_client
            .process_outgoing_action_with_receipt(
                state_hash,
                total_leaves,
                leaf_index,
                outgoing_action,
                proof,
            )
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

pub struct MirrorMemory<'mirror, 'api> {
    mirror: &'mirror Mirror<'api>,
    pages_hash: MaybeHashOf<MemoryPages>,
}

impl MirrorMemory<'_, '_> {
    pub async fn pages(&self) -> Result<Option<MemoryPages>> {
        self.mirror.memory_pages(self.pages_hash).await
    }

    pub async fn page_region(
        &self,
        page: GearPage,
    ) -> Result<Option<(HashOf<MemoryPagesRegion>, MemoryPagesRegion)>> {
        let Some(pages) = self.pages().await? else {
            return Ok(None);
        };
        let Some(region_hash) = pages[MemoryPages::page_region(page)].to_inner() else {
            return Ok(None);
        };
        let region = self
            .mirror
            .memory_page_region_unchecked(region_hash)
            .await?;

        Ok(Some((region_hash, region)))
    }

    pub async fn page_hash(&self, page: GearPage) -> Result<Option<HashOf<PageBuf>>> {
        let Some((_region_hash, region)) = self.page_region(page).await? else {
            return Ok(None);
        };

        Ok(region.as_inner().get(&page).copied())
    }

    pub async fn page(&self, page: GearPage) -> Result<Option<PageBuf>> {
        let Some(page_hash) = self.page_hash(page).await? else {
            return Ok(None);
        };

        self.mirror.page_data(page_hash).await.map(Some)
    }

    pub async fn byte(&self, offset: u32) -> Result<Option<u8>> {
        let page = GearPage::from_offset(offset);
        let Some(page_data) = self.page(page).await? else {
            return Ok(None);
        };
        let page_offset = (offset - page.offset()) as usize;

        Ok(page_data.get(page_offset).copied())
    }
}
