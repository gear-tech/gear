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

use super::ProcessingHandler;
use crate::{ProcessorError, Result};
use ethexe_common::{
    ScheduledTask,
    db::{CodesStorageRO, CodesStorageRW},
    events::{MirrorRequestEvent, RouterRequestEvent, WVaraRequestEvent},
    gear::{Origin, ValueClaim},
};
use ethexe_runtime_common::state::{Dispatch, Expiring, MailboxMessage, PayloadLookup};
use gear_core::{ids::ActorId, message::SuccessReplyReason};

impl ProcessingHandler {
    pub(crate) fn handle_router_event(&mut self, event: RouterRequestEvent) -> Result<()> {
        match event {
            RouterRequestEvent::ProgramCreated { actor_id, code_id } => {
                if self.db.original_code(code_id).is_none() {
                    return Err(ProcessorError::MissingCode(code_id));
                }

                if self.db.program_code_id(actor_id).is_some() {
                    return Err(ProcessorError::DuplicatedProgram(actor_id));
                }

                self.db.set_program_code_id(actor_id, code_id);

                self.transitions.register_new(actor_id);
            }
            RouterRequestEvent::ValidatorsCommittedForEra { .. }
            | RouterRequestEvent::CodeValidationRequested { .. }
            | RouterRequestEvent::ComputationSettingsChanged { .. }
            | RouterRequestEvent::StorageSlotChanged => {
                log::debug!("Handler not yet implemented: {event:?}");
            }
        };

        Ok(())
    }

    pub(crate) fn handle_mirror_event(
        &mut self,
        actor_id: ActorId,
        event: MirrorRequestEvent,
    ) -> Result<()> {
        if !self.transitions.is_program(&actor_id) {
            log::debug!("Received event from unrecognized mirror ({actor_id}): {event:?}");

            return Ok(());
        }

        match event {
            MirrorRequestEvent::ExecutableBalanceTopUpRequested { value } => {
                self.update_state(actor_id, |state, _, _| {
                    state.executable_balance += value;
                });
            }
            MirrorRequestEvent::MessageQueueingRequested {
                id,
                source,
                payload,
                value,
                call_reply,
            } => {
                self.update_state(actor_id, |state, storage, _| -> Result<()> {
                    let is_init = state.requires_init_message();

                    let dispatch = Dispatch::new(
                        storage,
                        id,
                        source,
                        payload,
                        value,
                        is_init,
                        Origin::Ethereum,
                        call_reply,
                    )?;

                    state
                        .canonical_queue
                        .modify_queue(storage, |queue| queue.queue(dispatch));

                    Ok(())
                })?;
            }
            MirrorRequestEvent::ReplyQueueingRequested {
                replied_to,
                source,
                payload,
                value,
            } => {
                self.update_state(actor_id, |state, storage, transitions| -> Result<()> {
                    let Some(Expiring {
                        value:
                            MailboxMessage {
                                value: claimed_value,
                                ..
                            },
                        expiry,
                    }) = state.mailbox_hash.modify_mailbox(storage, |mailbox| {
                        mailbox.remove_and_store_user_mailbox(storage, source, replied_to)
                    })
                    else {
                        return Ok(());
                    };

                    transitions.modify_transition(actor_id, |transition| {
                        transition.claims.push(ValueClaim {
                            message_id: replied_to,
                            destination: source,
                            value: claimed_value,
                        });
                    });

                    transitions.remove_task(
                        expiry,
                        &ScheduledTask::RemoveFromMailbox((actor_id, source), replied_to),
                    )?;

                    let reply = Dispatch::new_reply(
                        storage,
                        replied_to,
                        source,
                        payload,
                        value,
                        Origin::Ethereum,
                        false,
                    )?;

                    state
                        .canonical_queue
                        .modify_queue(storage, |queue| queue.queue(reply));

                    Ok(())
                })?;
            }
            MirrorRequestEvent::ValueClaimingRequested { claimed_id, source } => {
                self.update_state(actor_id, |state, storage, transitions| -> Result<()> {
                    let Some(Expiring {
                        value:
                            MailboxMessage {
                                value: claimed_value,
                                ..
                            },
                        expiry,
                    }) = state.mailbox_hash.modify_mailbox(storage, |mailbox| {
                        mailbox.remove_and_store_user_mailbox(storage, source, claimed_id)
                    })
                    else {
                        return Ok(());
                    };

                    transitions.modify_transition(actor_id, |transition| {
                        transition.claims.push(ValueClaim {
                            message_id: claimed_id,
                            destination: source,
                            value: claimed_value,
                        });
                    });

                    transitions.remove_task(
                        expiry,
                        &ScheduledTask::RemoveFromMailbox((actor_id, source), claimed_id),
                    )?;

                    let reply = Dispatch::reply(
                        claimed_id,
                        source,
                        PayloadLookup::empty(),
                        0,
                        SuccessReplyReason::Auto,
                        Origin::Ethereum,
                        false,
                    );

                    state
                        .canonical_queue
                        .modify_queue(storage, |queue| queue.queue(reply));

                    Ok(())
                })?;
            }
        };

        Ok(())
    }

    pub(crate) fn handle_wvara_event(&mut self, event: WVaraRequestEvent) {
        match event {
            WVaraRequestEvent::Transfer { from, to, value } => {
                if self.transitions.is_program(&to) && !self.transitions.is_program(&from) {
                    self.update_state(to, |state, _, _| state.balance += value);
                }
            }
        }
    }
}
