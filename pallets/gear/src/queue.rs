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
use core_processor::{common::PrechargedDispatch, ContextChargedForInstrumentation};

pub(crate) struct QueueStep<'a, T: Config, F> {
    pub block_config: &'a BlockConfig,
    pub ext_manager: &'a mut ExtManager<T>,
    pub gas_limit: GasBalanceOf<T>,
    pub dispatch: StoredDispatch,
    pub balance: u128,
    pub get_actor_data: F,
}

#[derive(Debug)]
pub(crate) enum QueueStepError {
    ActorData(PrechargedDispatch),
}

impl<'a, T, F> QueueStep<'a, T, F>
where
    T: Config,
    T::AccountId: Origin,
    F: FnOnce(
        PrechargedDispatch,
    ) -> Result<(PrechargedDispatch, Option<ExecutableActorData>), PrechargedDispatch>,
{
    pub(crate) fn execute(self) -> Result<Vec<JournalNote>, QueueStepError> {
        let Self {
            block_config,
            ext_manager,
            gas_limit,
            dispatch,
            balance,
            get_actor_data,
        } = self;

        let program_id = dispatch.destination();
        let dispatch_id = dispatch.id();

        // To start executing a message resources of a destination program should be
        // fetched from the storage.
        // The first step is to get program data so charge gas for the operation.
        let precharged_dispatch = match core_processor::precharge_for_program(
            block_config,
            GasAllowanceOf::<T>::get(),
            dispatch.into_incoming(gas_limit),
            program_id,
        ) {
            Ok(dispatch) => dispatch,
            Err(journal) => return Ok(journal),
        };

        let (precharged_dispatch, actor_data) =
            get_actor_data(precharged_dispatch).map_err(QueueStepError::ActorData)?;

        // The second step is to load instrumented binary code of the program but
        // first its correct length should be obtained.
        let context = match core_processor::precharge_for_code_length(
            block_config,
            precharged_dispatch,
            program_id,
            actor_data,
        ) {
            Ok(context) => context,
            Err(journal) => return Ok(journal),
        };

        // Load correct code length value.
        let code_id = context.actor_data().code_id;
        let code_len_bytes = T::CodeStorage::get_code_len(code_id).unwrap_or_else(|| {
            unreachable!(
                "Program '{:?}' exists so do code len '{:?}'",
                program_id, code_id
            )
        });

        // Adjust gas counters for fetching instrumented binary code.
        let context =
            match core_processor::precharge_for_code(block_config, context, code_len_bytes) {
                Ok(context) => context,
                Err(journal) => return Ok(journal),
            };

        // Load instrumented binary code from storage.
        let code = T::CodeStorage::get_code(code_id).unwrap_or_else(|| {
            unreachable!(
                "Program '{:?}' exists so do code '{:?}'",
                program_id, code_id
            )
        });

        // Reinstrument the code if necessary.
        let schedule = T::Schedule::get();
        let (code, context) =
            if code.instruction_weights_version() == schedule.instruction_weights.version {
                (code, ContextChargedForInstrumentation::from(context))
            } else {
                log::debug!("Re-instrumenting code for program '{:?}'", program_id);

                let context = match core_processor::precharge_for_instrumentation(
                    block_config,
                    context,
                    code.original_code_len(),
                ) {
                    Ok(context) => context,
                    Err(journal) => return Ok(journal),
                };

                (Pallet::<T>::reinstrument_code(code_id, &schedule), context)
            };

        // The last one thing is to load program memory. Adjust gas counters for memory pages.
        let context = match core_processor::precharge_for_memory(block_config, context) {
            Ok(context) => context,
            Err(journal) => return Ok(journal),
        };

        // Load program memory pages.
        ext_manager.insert_program_id_loaded_pages(program_id);

        let (random, bn) = T::Randomness::random(dispatch_id.as_ref());

        let journal = core_processor::process::<Ext>(
            block_config,
            (context, code, balance).into(),
            (random.encode(), bn.unique_saturated_into()),
        )
        .unwrap_or_else(|e| unreachable!("core-processor logic invalidated: {}", e));

        Ok(journal)
    }
}

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
            let dispatch_id = dispatch.id();
            let dispatch_reply = dispatch.reply_details().is_some();

            let balance = CurrencyOf::<T>::free_balance(&program_id.cast());

            let get_actor_data = |precharged_dispatch: PrechargedDispatch| {
                // At this point gas counters should be changed accordingly so fetch the program data.
                match Self::get_active_actor_data(program_id, dispatch_id, dispatch_reply) {
                    ActorResult::Data(data) => Ok((precharged_dispatch, data)),
                    ActorResult::Continue => Err(precharged_dispatch),
                }
            };

            let step = QueueStep {
                block_config: &block_config,
                ext_manager: &mut ext_manager,
                gas_limit,
                dispatch,
                balance: balance.unique_saturated_into(),
                get_actor_data,
            };
            match step.execute() {
                Ok(journal) => {
                    core_processor::handle_journal(journal, &mut ext_manager);
                }
                Err(QueueStepError::ActorData(precharged_dispatch)) => {
                    let (dispatch, journal) = precharged_dispatch.into_dispatch_and_note();
                    let (kind, message, context) = dispatch.into();
                    let dispatch =
                        StoredDispatch::new(kind, message.into_stored(program_id), context);

                    core_processor::handle_journal(journal, &mut ext_manager);

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
                }
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
                // (for example. moved from wait list in case of async init).
                // Also this branch is reachable when program sends a message to a terminated
                // program.
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
            code_id: program.code_hash.cast(),
            code_exports: program.code_exports,
            static_pages: program.static_pages,
            initialized: matches!(program.state, ProgramState::Initialized),
            pages_with_data: program.pages_with_data,
            gas_reservation_map: program.gas_reservation_map,
            memory_infix: program.memory_infix,
        }))
    }
}
