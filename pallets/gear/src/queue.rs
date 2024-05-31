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
use core_processor::{
    common::{LazyStorageAccess, ProgramInfo},
    PrepareError,
};

pub(crate) struct QueueStep<'a, T: Config> {
    pub block_config: &'a BlockConfig,
    pub gas_limit: GasBalanceOf<T>,
    pub dispatch: StoredDispatch,
    pub balance: u128,
}

pub(crate) struct StorageAccess<T>(pub PhantomData<T>);

impl<T: Config> LazyStorageAccess for StorageAccess<T>
where
    T::AccountId: Origin,
{
    fn program_info(&self, program_id: ProgramId) -> Option<ProgramInfo> {
        match ProgramStorageOf::<T>::get_program(program_id) {
            Some(Program::Active(program)) => Some(ProgramInfo {
                allocations: program.allocations,
                code_id: program.code_hash.into(),
                code_exports: program.code_exports,
                memory_infix: program.memory_infix,
                gas_reservation_map: program.gas_reservation_map,
                state: program.state,
            }),
            _ => None,
        }
    }

    fn code_len(&self, code_id: CodeId) -> Option<u32> {
        T::CodeStorage::get_code_len(code_id)
    }

    fn code(&self, code_id: CodeId) -> Option<InstrumentedCode> {
        T::CodeStorage::get_code(code_id)
    }

    fn need_reinstrumentation(&self, code: &InstrumentedCode) -> bool {
        code.instruction_weights_version() != T::Schedule::get().instruction_weights.version
    }

    fn reinstrument_code(&self, code_id: CodeId) -> Result<InstrumentedCode, CodeError> {
        Pallet::<T>::reinstrument_code(code_id, &T::Schedule::get())
    }
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

        let storage = StorageAccess::<T>(PhantomData);
        let program_id = dispatch.destination();
        let dispatch_id = dispatch.id();
        let dispatch = dispatch.into_incoming(gas_limit);

        let execution_context = match core_processor::prepare(
            &storage,
            block_config,
            GasAllowanceOf::<T>::get(),
            dispatch,
            program_id,
            balance,
        ) {
            Ok(ctx) => ctx,
            Err(PrepareError::Actor(err)) => return err.0,
            Err(PrepareError::System(err)) => {
                unreachable!("{err}")
            }
        };

        let (random, bn) = T::Randomness::random(dispatch_id.as_ref());

        core_processor::process::<Ext>(
            block_config,
            execution_context,
            (random.encode(), bn.unique_saturated_into()),
        )
        .unwrap_or_else(|e| unreachable!("{e}"))
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
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {e:?}"))
            {
                Some(d) => d,
                None => break,
            };

            // Querying gas limit. Fails in cases of `GasTree` invalidations.
            let gas_limit = GasHandlerOf::<T>::get_limit(dispatch.id())
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {e:?}"));

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
            if let Some(f) = builtin_dispatcher.lookup(&program_id) {
                core_processor::handle_journal(
                    builtin_dispatcher.run(f, dispatch, gas_limit),
                    &mut ext_manager,
                );
                continue;
            }

            let balance = CurrencyOf::<T>::free_balance(&program_id.cast());

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
