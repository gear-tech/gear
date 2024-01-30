// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
use common::ActiveProgram;
use core_processor::ContextChargedForInstrumentation;
use gear_core_errors::ErrorReplyReason;

pub(crate) struct QueueStep<'a, T: Config> {
    pub block_config: &'a BlockConfig,
    pub ext_manager: &'a mut ExtManager<T>,
    pub gas_limit: GasBalanceOf<T>,
    pub dispatch: StoredDispatch,
    pub balance: u128,
}

impl<T: Config> pallet::Pallet<T>
where
    T::AccountId: Origin,
{
    pub(crate) fn queue_step(queue_step: QueueStep<'_, T>) -> Vec<JournalNote> {
        let QueueStep {
            block_config,
            ext_manager,
            gas_limit,
            dispatch,
            balance,
        } = queue_step;

        let destination_id = dispatch.destination();
        let dispatch_id = dispatch.id();
        let dispatch_kind = dispatch.kind();

        // To start executing a message resources of a destination program should be
        // fetched from the storage.
        // The first step is to get program data so charge gas for the operation.
        let precharged_dispatch = match core_processor::precharge_for_program(
            block_config,
            GasAllowanceOf::<T>::get(),
            dispatch.into_incoming(gas_limit),
            destination_id,
        ) {
            Ok(dispatch) => dispatch,
            Err(journal) => return journal,
        };

        // Can't process messages for non-active program.
        let Some(program) = Self::get_active_program(destination_id) else {
            log::trace!("Message is sent to non-active program {:?}", destination_id);
            return core_processor::process_non_executable(
                precharged_dispatch,
                destination_id,
                ErrorReplyReason::InactiveProgram,
            );
        };

        if program.state == ProgramState::Initialized && dispatch_kind == DispatchKind::Init {
            // Panic is impossible, because gear protocol does not provide functionality
            // to send second init message to any already existing program.
            panic!("Init message is sent to already initialized program");
        }

        // If the destination program is uninitialized, then we allow
        // to process message, if it's a reply or the init message.
        // Otherwise, we appends message to the waiting init message list.
        if matches!(program.state, ProgramState::Uninitialized { message_id }
            if message_id != dispatch_id && dispatch_kind != DispatchKind::Reply)
        {
            let (dispatch, gas, _) = precharged_dispatch.into_parts();
            return vec![
                JournalNote::GasBurned {
                    message_id: dispatch.id(),
                    amount: gas.burned(),
                },
                JournalNote::WaitingInitMessage {
                    dispatch,
                    destination: destination_id,
                },
            ];
        }

        let actor_data = ExecutableActorData {
            allocations: program.allocations,
            code_id: program.code_hash.cast(),
            code_exports: program.code_exports,
            static_pages: program.static_pages,
            initialized: matches!(program.state, ProgramState::Initialized),
            pages_with_data: program.pages_with_data,
            gas_reservation_map: program.gas_reservation_map,
            memory_infix: program.memory_infix,
        };

        // The second step is to load instrumented binary code of the program but
        // first its correct length should be obtained.
        let context = match core_processor::precharge_for_code_length(
            block_config,
            precharged_dispatch,
            destination_id,
            actor_data,
        ) {
            Ok(context) => context,
            Err(journal) => return journal,
        };

        // Load correct code length value.
        let code_id = context.actor_data().code_id;
        let code_len_bytes = T::CodeStorage::get_code_len(code_id).unwrap_or_else(|| {
            unreachable!(
                "Program '{:?}' exists so do code len '{:?}'",
                destination_id, code_id
            )
        });

        // Adjust gas counters for fetching instrumented binary code.
        let context =
            match core_processor::precharge_for_code(block_config, context, code_len_bytes) {
                Ok(context) => context,
                Err(journal) => return journal,
            };

        // Load instrumented binary code from storage.
        let code = T::CodeStorage::get_code(code_id).unwrap_or_else(|| {
            unreachable!(
                "Program '{:?}' exists so do code '{:?}'",
                destination_id, code_id
            )
        });

        // Reinstrument the code if necessary.
        let schedule = T::Schedule::get();
        let (code, context) =
            if code.instruction_weights_version() == schedule.instruction_weights.version {
                (code, ContextChargedForInstrumentation::from(context))
            } else {
                log::debug!("Re-instrumenting code for program '{:?}'", destination_id);

                let context = match core_processor::precharge_for_instrumentation(
                    block_config,
                    context,
                    code.original_code_len(),
                ) {
                    Ok(context) => context,
                    Err(journal) => return journal,
                };

                let code = match Pallet::<T>::reinstrument_code(code_id, &schedule) {
                    Ok(code) => code,
                    Err(e) => {
                        log::debug!("Re-instrumentation error for code {:?}: {e:?}", code_id);
                        return core_processor::process_reinstrumentation_error(context);
                    }
                };

                (code, context)
            };

        // The last one thing is to load program memory. Adjust gas counters for memory pages.
        let context = match core_processor::precharge_for_memory(block_config, context) {
            Ok(context) => context,
            Err(journal) => return journal,
        };

        // Load program memory pages.
        ext_manager.insert_program_id_loaded_pages(destination_id);

        let (random, bn) = T::Randomness::random(dispatch_id.as_ref());

        core_processor::process::<Ext>(
            block_config,
            (context, code, balance).into(),
            (random.encode(), bn.unique_saturated_into()),
        )
        .unwrap_or_else(|e| unreachable!("core-processor logic invalidated: {}", e))
    }

    /// Message Queue processing.
    pub(crate) fn process_queue(mut ext_manager: ExtManager<T>) {
        Self::enable_lazy_pages();

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

            let balance = CurrencyOf::<T>::free_balance(&program_id.cast());

            let journal = Self::queue_step(QueueStep {
                block_config: &block_config,
                ext_manager: &mut ext_manager,
                gas_limit,
                dispatch,
                balance: balance.unique_saturated_into(),
            });

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

    fn get_active_program(program_id: ProgramId) -> Option<ActiveProgram<BlockNumberFor<T>>> {
        let Some(maybe_active_program) = ProgramStorageOf::<T>::get_program(program_id) else {
            // When an actor sends messages, which is intended to be added to the queue
            // it's destination existence is always checked. There are two cases this
            // doesn't happen:
            // 1. program tries to submit another program with non-existing code hash;
            // 2. program was being paused after message enqueued.
            return None;
        };

        match maybe_active_program {
            Program::Active(p) => Some(p),
            _ => {
                // Reaching this branch is possible when init message was processed with failure,
                // while other kind of messages were already in the queue/were added to the queue
                // (for example. moved from wait list in case of async init).
                // Also this branch is reachable when program sends a message to a terminated
                // program.
                log::trace!("Program '{program_id:?}' exists, but it isn't active");
                None
            }
        }
    }
}
