// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use super::*;
use crate::journal_builder::{JournalBuilder, JournalBuilderError};
use core_processor::common::PrechargedDispatch;

pub(crate) enum ActorResult {
    Continue,
    Data(Option<ExecutableActorData>),
}

impl<T: Config> pallet::Pallet<T>
where
    T::AccountId: Origin,
{
    /// Message Queue processing.
    pub(crate) fn process_queue(mut ext_manager: ExtManager<T>) {
        let block_config = Self::block_config();

        if T::DebugInfo::is_remap_id_enabled() {
            T::DebugInfo::remap_id();
        }

        let lazy_pages_enabled = Self::enable_lazy_pages();

        while QueueProcessingOf::<T>::allowed() {
            let dispatch = match QueueOf::<T>::dequeue()
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e))
            {
                Some(d) => d,
                None => break,
            };

            // Querying gas limit. Fails in cases of `GasTree` invalidations.
            let gas_limit = GasHandlerOf::<T>::get_limit(dispatch.id())
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            // Querying external id. Fails in cases of `GasTree` invalidations.
            let external = GasHandlerOf::<T>::get_external(dispatch.id())
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            log::debug!(
                "QueueProcessing message ({:?}): {:?} to {:?} / gas_limit: {}, gas_allowance: {}",
                dispatch.kind(),
                dispatch.id(),
                dispatch.destination(),
                gas_limit,
                GasAllowanceOf::<T>::get(),
            );

            let _guard = scopeguard::guard((), |_| {
                if T::DebugInfo::is_enabled() {
                    T::DebugInfo::do_snapshot();
                }

                if T::DebugInfo::is_remap_id_enabled() {
                    T::DebugInfo::remap_id();
                }
            });

            let program_id = dispatch.destination();
            let dispatch_id = dispatch.id();
            let dispatch_reply = dispatch.reply().is_some();

            let balance = CurrencyOf::<T>::free_balance(&<T::AccountId as Origin>::from_origin(
                program_id.into_origin(),
            ));

            let get_actor_data = |precharged_dispatch: PrechargedDispatch| {
                // At this point gas counters should be changed accordingly so fetch the program data.
                match Self::get_active_actor_data(program_id, dispatch_id, dispatch_reply) {
                    ActorResult::Data(data) => Ok((precharged_dispatch, data)),
                    ActorResult::Continue => {
                        let (dispatch, journal) = precharged_dispatch.into_dispatch_and_note();
                        let (kind, message, context) = dispatch.into();
                        let dispatch =
                            StoredDispatch::new(kind, message.into_stored(program_id), context);

                        // Adding id in on-init wake list.
                        ProgramStorageOf::<T>::waiting_init_append_message_id(
                            dispatch.destination(),
                            dispatch.id(),
                        );

                        Self::wait_dispatch(
                            dispatch,
                            None,
                            MessageWaitedSystemReason::ProgramIsNotInitialized.into_reason(),
                        );

                        Err(journal)
                    }
                }
            };

            let builder = JournalBuilder {
                block_config: &block_config,
                lazy_pages_enabled,
                ext_manager: &mut ext_manager,
                gas_limit,
                dispatch,
                balance: balance.unique_saturated_into(),
                external,
                get_actor_data,
            };
            match builder.build() {
                Ok(journal) => {
                    core_processor::handle_journal(journal, &mut ext_manager);
                }
                Err(JournalBuilderError::NoMemoryPages) => continue,
            }
        }

        let post_data: QueuePostProcessingData = ext_manager.into();
        let total_handled = DequeuedOf::<T>::get();

        if total_handled > 0 {
            Self::deposit_event(Event::MessagesDispatched {
                total: total_handled,
                statuses: post_data.dispatch_statuses,
                state_changes: post_data.state_changes,
            });
        }
    }

    pub(crate) fn get_active_actor_data(
        program_id: ProgramId,
        dispatch_id: MessageId,
        reply: bool,
    ) -> ActorResult {
        let Some(maybe_active_program) = ProgramStorageOf::<T>::get_program(program_id) else {
            // When an actor sends messages, which is intended to be added to the queue
            // it's destination existence is always checked. There are two cases this
            // doesn't happen:
            // 1. program tries to submit another program with non-existing code hash;
            // 2. program was being paused after message enqueued.
            return ActorResult::Data(None);
        };

        let program = match maybe_active_program {
            Program::Active(p) => p,
            _ => {
                // Reaching this branch is possible when init message was processed with failure,
                // while other kind of messages were already in the queue/were added to the queue
                // (for example. moved from wait list in case of async init)
                log::debug!("Program '{program_id:?}' is not active");
                return ActorResult::Data(None);
            }
        };

        if matches!(program.state, ProgramState::Uninitialized {message_id} if message_id != dispatch_id)
            && !reply
        {
            return ActorResult::Continue;
        }

        ActorResult::Data(Some(ExecutableActorData {
            allocations: program.allocations,
            code_id: CodeId::from_origin(program.code_hash),
            code_exports: program.code_exports,
            static_pages: program.static_pages,
            initialized: matches!(program.state, ProgramState::Initialized),
            pages_with_data: program.pages_with_data,
            gas_reservation_map: program.gas_reservation_map,
        }))
    }
}
