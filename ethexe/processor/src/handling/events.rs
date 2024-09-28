use anyhow::Result;
use ethexe_common::{
    mirror::RequestEvent as MirrorEvent,
    router::{RequestEvent as RouterEvent, ValueClaim},
    wvara::RequestEvent as WVaraEvent,
};
use ethexe_db::CodesStorage;
use ethexe_runtime_common::state::{Dispatch, Storage};
use gear_core::{
    ids::{prelude::MessageIdExt, ProgramId},
    message::{DispatchKind, ReplyDetails, SuccessReplyReason},
};
use gprimitives::{ActorId, CodeId, MessageId, H256};
use std::collections::BTreeMap;

use crate::Processor;

impl Processor {
    pub(crate) fn handle_router_event(
        &mut self,
        states: &mut BTreeMap<ProgramId, H256>,
        event: RouterEvent,
    ) -> Result<()> {
        match event {
            RouterEvent::ProgramCreated { actor_id, code_id } => {
                self.handle_new_program(actor_id, code_id)?;

                states.insert(actor_id, H256::zero());
            }
            RouterEvent::CodeValidationRequested { .. }
            | RouterEvent::BaseWeightChanged { .. }
            | RouterEvent::StorageSlotChanged
            | RouterEvent::ValidatorsSetChanged
            | RouterEvent::ValuePerWeightChanged { .. } => {
                log::debug!("Handler not yet implemented: {event:?}");
                return Ok(());
            }
        };

        Ok(())
    }

    pub(crate) fn handle_mirror_event(
        &mut self,
        states: &mut BTreeMap<ProgramId, H256>,
        value_claims: &mut BTreeMap<ProgramId, Vec<ValueClaim>>,
        actor_id: ProgramId,
        event: MirrorEvent,
    ) -> Result<()> {
        let Some(&state_hash) = states.get(&actor_id) else {
            log::debug!("Received event from unrecognized mirror ({actor_id}): {event:?}");

            return Ok(());
        };

        let new_state_hash = match event {
            MirrorEvent::ExecutableBalanceTopUpRequested { value } => {
                self.handle_executable_balance_top_up(state_hash, value)?
            }
            MirrorEvent::MessageQueueingRequested {
                id,
                source,
                payload,
                value,
            } => {
                let payload_hash = self.handle_payload(payload)?;

                let state = self
                    .db
                    .read_state(state_hash)
                    .ok_or_else(|| anyhow::anyhow!("program should exist"))?;

                let kind = if state.requires_init_message() {
                    DispatchKind::Init
                } else {
                    DispatchKind::Handle
                };

                let dispatch = Dispatch {
                    id,
                    kind,
                    source,
                    payload_hash,
                    value,
                    details: None,
                    context: None,
                };

                self.handle_message_queueing(state_hash, dispatch)?
            }
            MirrorEvent::ReplyQueueingRequested {
                replied_to,
                source,
                payload,
                value,
            } => {
                let Some((value_claim, state_hash)) =
                    self.handle_reply_queueing(state_hash, replied_to, source, payload, value)?
                else {
                    return Ok(());
                };

                value_claims.entry(actor_id).or_default().push(value_claim);

                state_hash
            }
            MirrorEvent::ValueClaimingRequested { claimed_id, source } => {
                let Some((value_claim, state_hash)) =
                    self.handle_value_claiming(state_hash, claimed_id, source)?
                else {
                    return Ok(());
                };

                value_claims.entry(actor_id).or_default().push(value_claim);

                state_hash
            }
        };

        states.insert(actor_id, new_state_hash);

        Ok(())
    }

    pub(crate) fn handle_wvara_event(
        &mut self,
        _states: &mut BTreeMap<ProgramId, H256>,
        event: WVaraEvent,
    ) -> Result<()> {
        match event {
            WVaraEvent::Transfer { .. } => {
                log::debug!("Handler not yet implemented: {event:?}");
                Ok(())
            }
        }
    }

    pub fn handle_executable_balance_top_up(
        &mut self,
        state_hash: H256,
        value: u128,
    ) -> Result<H256> {
        let mut state = self
            .db
            .read_state(state_hash)
            .ok_or_else(|| anyhow::anyhow!("program should exist"))?;

        state.executable_balance += value;

        Ok(self.db.write_state(state))
    }

    pub(crate) fn handle_reply_queueing(
        &mut self,
        state_hash: H256,
        mailboxed_id: MessageId,
        user_id: ActorId,
        payload: Vec<u8>,
        value: u128,
    ) -> Result<Option<(ValueClaim, H256)>> {
        self.handle_mailboxed_message_impl(
            state_hash,
            mailboxed_id,
            user_id,
            payload,
            value,
            SuccessReplyReason::Manual,
        )
    }

    pub(crate) fn handle_value_claiming(
        &mut self,
        state_hash: H256,
        mailboxed_id: MessageId,
        user_id: ActorId,
    ) -> Result<Option<(ValueClaim, H256)>> {
        self.handle_mailboxed_message_impl(
            state_hash,
            mailboxed_id,
            user_id,
            vec![],
            0,
            SuccessReplyReason::Auto,
        )
    }

    pub(crate) fn handle_mailboxed_message_impl(
        &mut self,
        state_hash: H256,
        mailboxed_id: MessageId,
        user_id: ActorId,
        payload: Vec<u8>,
        value: u128,
        reply_reason: SuccessReplyReason,
    ) -> Result<Option<(ValueClaim, H256)>> {
        let mut state = self
            .db
            .read_state(state_hash)
            .ok_or_else(|| anyhow::anyhow!("program should exist"))?;

        let mut mailbox = state.mailbox_hash.with_hash_or_default(|hash| {
            self.db.read_mailbox(hash).expect("Failed to read mailbox")
        });

        let entry = mailbox.entry(user_id).or_default();

        let Some((claimed_value, _expiration)) = entry.remove(&mailboxed_id) else {
            return Ok(None);
        };

        state.mailbox_hash = self.db.write_mailbox(mailbox).into();

        let claim = ValueClaim {
            message_id: mailboxed_id,
            destination: user_id,
            value: claimed_value,
        };

        let mut queue = state
            .queue_hash
            .with_hash_or_default(|hash| self.db.read_queue(hash).expect("Failed to read queue"));

        let payload_hash = self.handle_payload(payload)?;

        let dispatch = Dispatch {
            id: MessageId::generate_reply(mailboxed_id),
            kind: DispatchKind::Reply,
            source: user_id,
            payload_hash,
            value,
            details: Some(ReplyDetails::new(mailboxed_id, reply_reason.into()).into()),
            context: None,
        };

        queue.push_back(dispatch);

        state.queue_hash = self.db.write_queue(queue).into();

        Ok(Some((claim, self.db.write_state(state))))
    }

    pub fn handle_new_program(&mut self, program_id: ProgramId, code_id: CodeId) -> Result<()> {
        if self.db.original_code(code_id).is_none() {
            anyhow::bail!("code existence should be checked on smart contract side");
        }

        if self.db.program_code_id(program_id).is_some() {
            anyhow::bail!("program duplicates should be checked on smart contract side");
        }

        self.db.set_program_code_id(program_id, code_id);

        Ok(())
    }
}
