// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
use core_processor::{
    ContextCharged, ProcessExecutionContext,
    common::{DispatchResult, SuccessfulDispatchResultKind},
};
use gear_core::{code::InstrumentedCodeAndMetadata, program::ProgramState};

pub(crate) struct QueueStep<'a, T: Config> {
    pub block_config: &'a BlockConfig,
    pub gas_limit: GasBalanceOf<T>,
    pub dispatch: StoredDispatch,
    pub balance: u128,
}

impl<T: Config> pallet::Pallet<T>
where
    T::AccountId: Origin,
{
    pub(crate) fn run_queue_step(queue_step: QueueStep<'_, T>) -> Vec<JournalNote> {
        let QueueStep {
            block_config,
            gas_limit,
            dispatch,
            balance,
        } = queue_step;

        let destination_id = dispatch.destination();
        let dispatch_id = dispatch.id();
        let dispatch_kind = dispatch.kind();

        let context = ContextCharged::new(
            destination_id,
            dispatch.into_incoming(gas_limit),
            GasAllowanceOf::<T>::get(),
        );

        // To start executing a message resources of a destination program should be
        // fetched from the storage.
        // The first step is to get program data so charge gas for the operation.
        let context = match context.charge_for_program(block_config) {
            Ok(context) => context,
            Err(journal) => return journal,
        };

        let program = match ProgramStorageOf::<T>::get_program(destination_id) {
            Some(Program::Active(program)) => program,
            Some(Program::Terminated(_)) => {
                log::trace!(
                    "Message {dispatch_id} is sent to failed init program {destination_id}"
                );
                return core_processor::process_failed_init(context);
            }
            Some(Program::Exited(program_id)) => {
                log::trace!("Message {dispatch_id} is sent to exited program {destination_id}");
                return core_processor::process_program_exited(context, program_id);
            }
            None => {
                log::trace!(
                    "Message {dispatch_id} is sent to nonexistent program {destination_id}"
                );
                return core_processor::process_code_not_exists(context);
            }
        };

        if program.state == ProgramState::Initialized && dispatch_kind == DispatchKind::Init {
            // Panic is impossible, because gear protocol does not provide functionality
            // to send second init message to any already existing program.
            let err_msg = format!(
                "run_queue_step: got init message for already initialized program. \
                Current init message id - {dispatch_id:?}, already initialized program id - {destination_id:?}."
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        }

        // If the destination program is uninitialized, then we allow
        // to process message, if it's a reply (async init case) or init message.
        // Otherwise, we return error reply.
        if matches!(program.state, ProgramState::Uninitialized { message_id }
            if message_id != dispatch_id && dispatch_kind != DispatchKind::Reply)
        {
            if dispatch_kind == DispatchKind::Init {
                // Panic is impossible, because gear protocol does not provide functionality
                // to send second init message to any existing program.
                let err_msg = format!(
                    "run_queue_step: got init message which is not the first init message to the program. \
                    Current init message id - {dispatch_id:?}, original init message id - {dispatch_id}, program - {destination_id:?}.",
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            }

            return core_processor::process_uninitialized(context);
        }

        // Adjust gas counters for fetching code metadata.
        let context = match context.charge_for_code_metadata(block_config) {
            Ok(context) => context,
            Err(journal) => return journal,
        };

        let code_id = program.code_id;

        // The second step is to load code metadata
        let code_metadata = T::CodeStorage::get_code_metadata(code_id).unwrap_or_else(|| {
            let err_msg = format!(
                "run_queue_step: failed to get code metadata for the existing program. \
                Program id -'{destination_id:?}', Code id - '{code_id:?}'."
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });

        // TODO: This is an early check, it should be moved after re-instrumentation, in case re-instrumentation will change/add exports
        if !code_metadata.exports().contains(&dispatch_kind) {
            let (destination_id, dispatch, gas_counter, _) = context.into_parts();

            return core_processor::process_success(
                SuccessfulDispatchResultKind::Success,
                DispatchResult::success(&dispatch, destination_id, gas_counter.to_amount()),
                dispatch,
            );
        }

        let schedule = T::Schedule::get();

        // Check if the code needs to be reinstrumented.
        let needs_reinstrumentation = match code_metadata.instrumentation_status() {
            InstrumentationStatus::NotInstrumented => {
                log::debug!(
                    "Instrumented code doesn't exists for program '{destination_id:?}' \
                     we need to instrument it with instructions weights version {}",
                    schedule.instruction_weights.version
                );

                true
            }
            InstrumentationStatus::Instrumented { version, .. } => {
                version != schedule.instruction_weights.version
            }
            InstrumentationStatus::InstrumentationFailed { version } => {
                if version == schedule.instruction_weights.version {
                    log::debug!(
                        "Re-instrumentation already failed for program '{destination_id:?}' \
                        with instructions weights version {version}"
                    );

                    return core_processor::process_instrumentation_failed(context);
                }

                true
            }
        };

        // Reinstrument the code if necessary.
        let (instrumented_code, code_metadata, context) = if needs_reinstrumentation {
            log::debug!("Re-instrumenting code for program '{destination_id:?}'");

            let context = match context
                .charge_for_original_code(block_config, code_metadata.original_code_len())
            {
                Ok(code) => code,
                Err(journal) => return journal,
            };

            // TODO: Instrumentation cost should have "write-to-storage" cost inside
            let context = match context
                .charge_for_instrumentation(block_config, code_metadata.original_code_len())
            {
                Ok(code) => code,
                Err(journal) => return journal,
            };

            let instrumented_code_and_metadata =
                match Pallet::<T>::reinstrument_code(code_id, code_metadata, &schedule) {
                    Ok(code_and_metadata) => code_and_metadata,
                    Err(e) => {
                        log::debug!("Re-instrumentation error for code {code_id:?}: {e:?}");
                        return core_processor::process_reinstrumentation_error(context);
                    }
                };

            (
                instrumented_code_and_metadata.instrumented_code,
                instrumented_code_and_metadata.metadata,
                context,
            )
        } else {
            let instrumented_code_len = code_metadata.instrumented_code_len().unwrap_or_else(|| {
                let err_msg = format!(
                    "run_queue_step: code metadata for the existing program does not contain \
                    instrumented code length. Program id -'{destination_id:?}', Code id - '{code_id:?}'."
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

            // Adjust gas counters for fetching instrumented binary code.
            let context =
                match context.charge_for_instrumented_code(block_config, instrumented_code_len) {
                    Ok(context) => context,
                    Err(journal) => return journal,
                };

            let code = T::CodeStorage::get_instrumented_code(code_id).unwrap_or_else(|| {
                // `Program` exists, so instrumented code must exist as well.
                let err_msg = format!(
                    "run_queue_step: failed to get code for the existing program. \
                    Program id -'{destination_id:?}', Code id - '{code_id:?}'."
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

            (code, code_metadata, context)
        };

        let context =
            match context.charge_for_allocations(block_config, program.allocations_tree_len) {
                Ok(context) => context,
                Err(journal) => return journal,
            };

        let allocations = if program.allocations_tree_len != 0 {
            {
                ProgramStorageOf::<T>::allocations(destination_id).unwrap_or_else(|| {
                unreachable!(
                    "`allocations_tree_len` {} is not zero, so program {destination_id:?} must have allocations",
                    program.allocations_tree_len,
                )
            })
            }
        } else {
            Default::default()
        };

        let actor_data = ExecutableActorData {
            allocations,
            gas_reservation_map: program.gas_reservation_map,
            memory_infix: program.memory_infix,
        };

        // The last one thing is to load program memory. Adjust gas counters for memory pages.
        let context = match context.charge_for_module_instantiation(
            block_config,
            actor_data,
            instrumented_code.instantiated_section_sizes(),
            &code_metadata,
        ) {
            Ok(context) => context,
            Err(journal) => return journal,
        };

        let (random, bn) = T::Randomness::random(dispatch_id.as_ref());

        core_processor::process::<Ext>(
            block_config,
            ProcessExecutionContext::new
            (context, InstrumentedCodeAndMetadata{instrumented_code, metadata: code_metadata}, balance),
            (random.encode(), bn.unique_saturated_into()),
        )
        .unwrap_or_else(|e| {
            let err_msg = format!(
                "run_queue_step: failed processing message. Message id - {dispatch_id}, program id - {destination_id}. \
                Got error - {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        })
    }

    /// Message Queue processing.
    pub(crate) fn process_queue(mut ext_manager: ExtManager<T>) {
        Self::enable_lazy_pages();

        let block_config = Self::block_config();

        if T::DebugInfo::is_remap_id_enabled() {
            T::DebugInfo::remap_id();
        }

        while QueueProcessingOf::<T>::allowed() {
            let dispatch = match QueueOf::<T>::dequeue() {
                Ok(Some(d)) => d,
                Ok(None) => break,
                Err(e) => {
                    let err_msg =
                        format!("process_queue: failed dequeuing message. Got error - {e:?}");

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}")
                }
            };

            // Querying gas limit. Fails in cases of `GasTree` invalidations.
            let gas_limit = GasHandlerOf::<T>::get_limit(dispatch.id())
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "process_queue: failed getting message gas limit. Message id - {}. Got error - {e:?}.",
                        dispatch.id()
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}")
                });

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

            // If the dispatch destination (a.k.a. `program_id`) resolves to some `handle` function
            // of a builtin actor, we handle the dispatch as a builtin actor dispatch.
            // Otherwise we proceed with the regular flow.
            let builtin_dispatcher = ext_manager.builtins();
            if let Some(info) = builtin_dispatcher.lookup(&program_id) {
                core_processor::handle_journal(
                    builtin_dispatcher.run(info, dispatch, gas_limit),
                    &mut ext_manager,
                );
                continue;
            }

            let balance = <CurrencyOf<T> as fungible::Inspect<T::AccountId>>::reducible_balance(
                &program_id.cast(),
                Preservation::Expendable,
                Fortitude::Polite,
            );

            let journal = Self::run_queue_step(QueueStep {
                block_config: &block_config,
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
}
