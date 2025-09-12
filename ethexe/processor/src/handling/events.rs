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
    NextEraValidators, ScheduledTask,
    db::{CodesStorageRead, CodesStorageWrite, OnChainStorageRead, OnChainStorageWrite},
    era_from_ts,
    events::{MirrorRequestEvent, RouterRequestEvent, WVaraRequestEvent},
    gear::{Origin, ValueClaim},
};
use ethexe_runtime_common::state::{Dispatch, Expiring, MailboxMessage, PayloadLookup};
use gear_core::{ids::ActorId, message::SuccessReplyReason};
use sp_wasm_interface::anyhow::anyhow;
use std::cmp::Ordering;

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
            RouterRequestEvent::NextEraValidatorsCommitted { era_index } => {
                let timelines = self.db.gear_exe_timelines().ok_or(anyhow!(""))?;
                let header = self.db.block_header(self.block_hash).ok_or(anyhow!(""))?;
                let block_era = era_from_ts(header.timestamp, timelines.genesis_ts, timelines.era);

                match era_index.cmp(&block_era) {
                    Ordering::Less => {
                        unimplemented!("Receive an outdated validators commitment")
                    }
                    Ordering::Equal => {
                        // This case happen, when commitment applies on Ethereum in the next era, but sent in previous.
                        // Iterate through parent blocks and found the elected validators in previous eras.
                        let mut parent_hash = header.parent_hash;
                        loop {
                            let parent = self.db.block_header(parent_hash).ok_or(anyhow!(""))?;
                            let parent_era =
                                era_from_ts(parent.timestamp, timelines.genesis_ts, timelines.era);

                            if parent_era == block_era {
                                // Still in current era, go next
                                parent_hash = parent.parent_hash;
                                continue;
                            }

                            // In next era.
                            let parent_validators_info =
                                self.db.validators_info(parent_hash).ok_or(anyhow!(""))?;

                            let NextEraValidators::Elected(elected_validators) =
                                parent_validators_info.next
                            else {
                                // Skip block if `next` validators not in `Elected` state
                                parent_hash = parent.parent_hash;
                                continue;
                            };
                            // Found elected validators. Update the old propagated validators set.
                            let mut validators_info = self
                                .db
                                .validators_info(self.block_hash)
                                .ok_or(anyhow!(""))?;
                            validators_info.current = elected_validators;
                            self.db
                                .set_validators_info(self.block_hash, validators_info);
                            break;
                        }
                    }
                    Ordering::Greater => {
                        debug_assert!(era_index == block_era + 1);

                        // The successful variant.
                        let mut validators_info = self
                            .db
                            .validators_info(self.block_hash)
                            .ok_or(anyhow!(""))?;

                        match validators_info.next.clone() {
                            NextEraValidators::Elected(elected_validators) => {
                                // Switch from `Elected` state to `Committed`
                                validators_info.next =
                                    NextEraValidators::Committed(elected_validators);
                                self.db
                                    .set_validators_info(self.block_hash, validators_info);
                            }
                            NextEraValidators::Committed(..) => {
                                log::warn!(
                                    "Validators in {block_era} are already committed, but receive another commitment"
                                );
                            }
                            NextEraValidators::Unknown => {
                                log::error!("Receive validators commitment without election");
                            }
                        }
                    }
                };
            }
            RouterRequestEvent::CodeValidationRequested { .. }
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
                        .queue
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
                        .queue
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
                        .queue
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
