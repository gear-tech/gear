// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
                "QueueProcessing message: {:?} to {:?} / gas_limit: {}, gas_allowance: {}",
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
            let precharged_dispatch = match core_processor::precharge(
                &block_config,
                GasAllowanceOf::<T>::get(),
                dispatch.into_incoming(gas_limit),
                program_id,
            ) {
                PrechargeResult::Ok(d) => d,
                PrechargeResult::Error(journal) => {
                    core_processor::handle_journal(journal, &mut ext_manager);

                    continue;
                }
            };

            let active_actor_data =
                match Self::get_active_actor_data(program_id, dispatch_id, dispatch_reply) {
                    ActorResult::Data(d) => d,
                    ActorResult::Continue => {
                        let (dispatch, journal) = precharged_dispatch.into_dispatch_and_note();
                        let (kind, message, context) = dispatch.into();
                        let dispatch =
                            StoredDispatch::new(kind, message.into_stored(program_id), context);

                        core_processor::handle_journal(journal, &mut ext_manager);

                        // Adding id in on-init wake list.
                        common::waiting_init_append_message_id(
                            dispatch.destination(),
                            dispatch.id(),
                        );

                        Self::wait_dispatch(
                            dispatch,
                            None,
                            MessageWaitedSystemReason::ProgramIsNotInitialized.into_reason(),
                        );

                        continue;
                    }
                };

            let balance = CurrencyOf::<T>::free_balance(&<T::AccountId as Origin>::from_origin(
                program_id.into_origin(),
            ))
            .unique_saturated_into();

            let message_execution_context = MessageExecutionContext {
                actor: Actor {
                    balance,
                    destination_program: program_id,
                    executable_data: active_actor_data,
                },
                precharged_dispatch,
                origin: ProgramId::from_origin(external.into_origin()),
                subsequent_execution: ext_manager.program_pages_loaded(&program_id),
            };

            let journal = match core_processor::prepare(&block_config, message_execution_context) {
                PrepareResult::Ok(context) => {
                    let memory_pages = match Self::get_and_track_memory_pages(
                        &mut ext_manager,
                        program_id,
                        &context.actor_data().pages_with_data,
                    ) {
                        None => continue,
                        Some(m) => m,
                    };

                    let code = Self::get_code(context.actor_data().code_id, program_id)
                        .unwrap_or_else(|| unreachable!("Program exists so do code"));
                    let (random, bn) = T::Randomness::random(dispatch_id.as_ref());
                    core_processor::process::<Ext, ExecutionEnvironment>(
                        &block_config,
                        (context, program_id, code).into(),
                        (random.encode(), bn.unique_saturated_into()),
                        memory_pages,
                    )
                }
                PrepareResult::WontExecute(journal) | PrepareResult::Error(journal) => journal,
            };

            core_processor::handle_journal(journal, &mut ext_manager);
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
        let maybe_active_program = match common::get_program(program_id.into_origin()) {
            Some(p) => p,
            None => {
                // When an actor sends messages, which is intended to be added to the queue
                // it's destination existence is always checked. The only case this doesn't
                // happen is when program tries to submit another program with non-existing
                // code hash. That's the only known case for reaching that branch.
                //
                // However there is another case with pausing program, but this API is unstable currently.
                return ActorResult::Data(None);
            }
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
            code_length_bytes: program.code_length_bytes,
            static_pages: program.static_pages,
            initialized: matches!(program.state, ProgramState::Initialized),
            pages_with_data: program.pages_with_data,
            gas_reservation_map: program.gas_reservation_map,
        }))
    }
}
