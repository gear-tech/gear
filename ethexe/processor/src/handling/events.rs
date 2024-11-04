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

use crate::Processor;
use anyhow::{anyhow, ensure, Result};
use ethexe_common::{
    mirror::RequestEvent as MirrorEvent,
    router::{RequestEvent as RouterEvent, ValueClaim},
    wvara::RequestEvent as WVaraEvent,
};
use ethexe_db::{CodesStorage, ScheduledTask};
use ethexe_runtime_common::{
    state::{ComplexStorage as _, Dispatch, Storage},
    InBlockTransitions,
};
use gear_core::{
    ids::ProgramId,
    message::{DispatchKind, SuccessReplyReason},
};
use gprimitives::{ActorId, CodeId, MessageId, H256};

impl Processor {
    pub(crate) fn handle_router_event(
        &mut self,
        in_block_transitions: &mut InBlockTransitions,
        event: RouterEvent,
    ) -> Result<()> {
        match event {
            RouterEvent::ProgramCreated { actor_id, code_id } => {
                self.handle_new_program(actor_id, code_id)?;
                in_block_transitions.register_new(actor_id);
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
        in_block_transitions: &mut InBlockTransitions,
        actor_id: ProgramId,
        event: MirrorEvent,
    ) -> Result<()> {
        let Some(state_hash) = in_block_transitions.state_of(&actor_id) else {
            log::debug!("Received event from unrecognized mirror ({actor_id}): {event:?}");

            return Ok(());
        };

        match event {
            MirrorEvent::ExecutableBalanceTopUpRequested { value } => {
                let new_state_hash = self.handle_executable_balance_top_up(state_hash, value)?;
                in_block_transitions
                    .modify_state(actor_id, new_state_hash)
                    .ok_or_else(|| anyhow!("failed to modify state of recognized program"))?;
            }
            MirrorEvent::MessageQueueingRequested {
                id,
                source,
                payload,
                value,
            } => {
                let payload_hash = self.db.store_payload(payload)?;

                let state = self
                    .db
                    .read_state(state_hash)
                    .ok_or_else(|| anyhow!("program should exist"))?;

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

                let new_state_hash = self.handle_message_queueing(state_hash, dispatch)?;
                in_block_transitions
                    .modify_state(actor_id, new_state_hash)
                    .ok_or_else(|| anyhow!("failed to modify state of recognized program"))?;
            }
            MirrorEvent::ReplyQueueingRequested {
                replied_to,
                source,
                payload,
                value,
            } => {
                if let Some((value_claim, expiry, new_state_hash)) =
                    self.handle_reply_queueing(state_hash, replied_to, source, payload, value)?
                {
                    in_block_transitions
                        .modify_transition(actor_id, |state_hash, transition| {
                            *state_hash = new_state_hash;
                            transition.claims.push(value_claim);
                        })
                        .ok_or_else(|| anyhow!("failed to modify state of recognized program"))?;

                    in_block_transitions.remove_task(
                        expiry,
                        &ScheduledTask::RemoveFromMailbox((actor_id, source), replied_to),
                    )?;
                }
            }
            MirrorEvent::ValueClaimingRequested { claimed_id, source } => {
                if let Some((value_claim, expiry, new_state_hash)) =
                    self.handle_value_claiming(state_hash, claimed_id, source)?
                {
                    in_block_transitions
                        .modify_transition(actor_id, |state_hash, transition| {
                            *state_hash = new_state_hash;
                            transition.claims.push(value_claim);
                        })
                        .ok_or_else(|| anyhow!("failed to modify state of recognized program"))?;

                    in_block_transitions.remove_task(
                        expiry,
                        &ScheduledTask::RemoveFromMailbox((actor_id, source), claimed_id),
                    )?;
                }
            }
        };

        Ok(())
    }

    pub(crate) fn handle_wvara_event(
        &mut self,
        in_block_transitions: &mut InBlockTransitions,
        event: WVaraEvent,
    ) -> Result<()> {
        match event {
            WVaraEvent::Transfer { from, to, value } => {
                if let Some(state_hash) = in_block_transitions.state_of(&to) {
                    if in_block_transitions.state_of(&from).is_none() {
                        let new_state_hash = self.db.mutate_state(state_hash, |_, state| {
                            state.balance += value;
                            Ok(())
                        })?;

                        in_block_transitions
                            .modify_state(to, new_state_hash)
                            .expect("queried above so infallible here");
                    }
                }

                Ok(())
            }
        }
    }

    pub fn handle_executable_balance_top_up(
        &mut self,
        state_hash: H256,
        value: u128,
    ) -> Result<H256> {
        self.db.mutate_state(state_hash, |_, state| {
            state.executable_balance += value;
            Ok(())
        })
    }

    pub(crate) fn handle_reply_queueing(
        &mut self,
        state_hash: H256,
        mailboxed_id: MessageId,
        user_id: ActorId,
        payload: Vec<u8>,
        value: u128,
    ) -> Result<Option<(ValueClaim, u32, H256)>> {
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
    ) -> Result<Option<(ValueClaim, u32, H256)>> {
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
    ) -> Result<Option<(ValueClaim, u32, H256)>> {
        self.db
            .mutate_state_returning(state_hash, |db, state| {
                let Some(((claimed_value, expiry), mailbox_hash)) =
                    db.modify_mailbox_if_changed(state.mailbox_hash.clone(), |mailbox| {
                        let local_mailbox = mailbox.get_mut(&user_id)?;
                        let claimed_value = local_mailbox.remove(&mailboxed_id)?;

                        if local_mailbox.is_empty() {
                            mailbox.remove(&user_id);
                        }

                        Some(claimed_value)
                    })?
                else {
                    return Ok(None);
                };

                state.mailbox_hash = mailbox_hash;

                let payload_hash = db.store_payload(payload)?;
                let reply =
                    Dispatch::reply(mailboxed_id, user_id, payload_hash, value, reply_reason);

                state.queue_hash =
                    db.modify_queue(state.queue_hash.clone(), |queue| queue.push_back(reply))?;

                Ok(Some((
                    ValueClaim {
                        message_id: mailboxed_id,
                        destination: user_id,
                        value: claimed_value,
                    },
                    expiry,
                )))
            })
            .map(|(claim_with_expiry, hash)| {
                if claim_with_expiry.is_none() {
                    debug_assert_eq!(hash, state_hash);
                }
                claim_with_expiry.map(|(claim, expiry)| (claim, expiry, hash))
            })
    }

    pub fn handle_new_program(&mut self, program_id: ProgramId, code_id: CodeId) -> Result<()> {
        ensure!(
            self.db.original_code(code_id).is_some(),
            "code existence must be checked on router"
        );

        ensure!(
            self.db.program_code_id(program_id).is_none(),
            "program duplicates must be checked on router"
        );

        self.db.set_program_code_id(program_id, code_id);

        Ok(())
    }
}
