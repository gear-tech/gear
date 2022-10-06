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

use crate::{
    manager::ExtManager, Config, CurrencyOf, Event, GasAllowanceOf, GasHandlerOf,
    GearProgramPallet, Pallet, QueueOf, SentOf, TaskPoolOf, WaitlistOf,
};
use common::{
    event::*,
    scheduler::{ScheduledTask, TaskPool},
    storage::*,
    CodeStorage, GasTree, Origin, Program,
};
use core_processor::common::{DispatchOutcome as CoreDispatchOutcome, JournalHandler};
use frame_support::{
    sp_runtime::Saturating,
    traits::{Currency, ExistenceRequirement, ReservableCurrency},
};
use frame_system::Pallet as SystemPallet;
use gear_core::{
    ids::{CodeId, MessageId, ProgramId},
    memory::{PageBuf, PageNumber},
    message::{Dispatch, StoredDispatch},
};
use sp_runtime::traits::{UniqueSaturatedInto, Zero};

use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    prelude::*,
};

impl<T: Config> JournalHandler for ExtManager<T>
where
    T::AccountId: Origin,
{
    fn message_dispatched(
        &mut self,
        message_id: MessageId,
        source: ProgramId,
        outcome: CoreDispatchOutcome,
    ) {
        use CoreDispatchOutcome::*;

        let wake_waiting_init_msgs = |p_id: ProgramId| {
            common::waiting_init_take_messages(p_id)
                .into_iter()
                .for_each(|m_id| {
                    if let Some(m) = Pallet::<T>::wake_dispatch(
                        p_id,
                        m_id,
                        MessageWokenSystemReason::ProgramGotInitialized.into_reason(),
                    ) {
                        QueueOf::<T>::queue(m)
                            .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
                    } else {
                        log::error!("Cannot find message in wl")
                    }
                })
        };

        let status = match outcome {
            Exit { program_id } => {
                log::trace!("Dispatch outcome exit: {:?}", message_id);

                Pallet::<T>::deposit_event(Event::ProgramChanged {
                    id: program_id,
                    change: ProgramChangeKind::Inactive,
                });

                DispatchStatus::Success
            }
            Success => {
                log::trace!("Dispatch outcome success: {:?}", message_id);

                DispatchStatus::Success
            }
            MessageTrap { program_id, trap } => {
                log::trace!("Dispatch outcome trap: {:?}", message_id);
                log::debug!(
                    "🪤 Program {} terminated with a trap: {}",
                    program_id.into_origin(),
                    trap
                );

                DispatchStatus::Failed
            }
            InitSuccess { program_id, .. } => {
                log::trace!(
                    "Dispatch ({:?}) init success for program {:?}",
                    message_id,
                    program_id
                );

                wake_waiting_init_msgs(program_id);
                common::set_program_initialized(program_id.into_origin());

                // TODO: replace this temporary (zero) value for expiration
                // block number with properly calculated one
                // (issues #646 and #969).
                Pallet::<T>::deposit_event(Event::ProgramChanged {
                    id: program_id,
                    change: ProgramChangeKind::Active {
                        expiration: T::BlockNumber::zero(),
                    },
                });

                DispatchStatus::Success
            }
            InitFailure {
                program_id, origin, ..
            } => {
                log::trace!(
                    "Dispatch ({:?}) init failure for program {:?}",
                    message_id,
                    program_id
                );

                // Some messages addressed to the program could be processed
                // in the queue before init message. For example, that could
                // happen when init message had more gas limit then rest block
                // gas allowance, but a dispatch message to the program was
                // dequeued. The other case is async init.
                wake_waiting_init_msgs(program_id);

                common::set_program_terminated_status(program_id.into_origin(), origin)
                    .expect("Only active program can cause init failure");

                let program_id = <T::AccountId as Origin>::from_origin(program_id.into_origin());

                let balance = CurrencyOf::<T>::free_balance(&program_id);
                let destination = Pallet::<T>::inheritor_for(origin);
                let destination = <T::AccountId as Origin>::from_origin(destination.into_origin());

                if !balance.is_zero() {
                    CurrencyOf::<T>::transfer(
                        &program_id,
                        &destination,
                        balance,
                        ExistenceRequirement::AllowDeath,
                    )
                    .unwrap_or_else(|e| unreachable!("Failed to transfer value: {:?}", e));
                }

                DispatchStatus::Failed
            }
            CoreDispatchOutcome::NoExecution => {
                log::trace!("Dispatch ({:?}) for program wasn't executed", message_id);

                DispatchStatus::NotExecuted
            }
        };

        if self.check_user_id(&source) {
            self.dispatch_statuses.insert(message_id, status);
        }
    }

    fn gas_burned(&mut self, message_id: MessageId, amount: u64) {
        log::debug!("Burned: {:?} from: {:?}", amount, message_id);

        GasAllowanceOf::<T>::decrease(amount);

        Pallet::<T>::spend_gas(message_id, amount)
    }

    fn exit_dispatch(&mut self, id_exited: ProgramId, value_destination: ProgramId) {
        let reason = MessageWokenSystemReason::ProgramGotInitialized.into_reason();

        WaitlistOf::<T>::drain_key(id_exited).for_each(|entry| {
            let message = Pallet::<T>::wake_dispatch_requirements(entry, reason.clone());

            QueueOf::<T>::queue(message)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
        });

        let _ = common::waiting_init_take_messages(id_exited);

        let id_exited = id_exited.into_origin();

        common::set_program_exited_status(id_exited, value_destination)
            .expect("`exit` can be called only from active program; qed");

        let program_account = &<T::AccountId as Origin>::from_origin(id_exited);
        let balance = CurrencyOf::<T>::free_balance(program_account);

        let destination = Pallet::<T>::inheritor_for(value_destination);
        let destination = <T::AccountId as Origin>::from_origin(destination.into_origin());

        if !balance.is_zero() {
            CurrencyOf::<T>::transfer(
                program_account,
                &destination,
                balance,
                ExistenceRequirement::AllowDeath,
            )
            .unwrap_or_else(|e| unreachable!("Failed to transfer value: {:?}", e));
        }
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        Pallet::<T>::consume_message(message_id)
    }

    fn send_dispatch(&mut self, message_id: MessageId, dispatch: Dispatch, delay: u32) {
        let to_user = self.check_user_id(&dispatch.destination());

        if !delay.is_zero() {
            log::debug!("Sending delayed for {delay} blocks dispatch");
            Pallet::<T>::send_delayed_dispatch(message_id, dispatch, delay, to_user)
        } else if !to_user {
            let gas_limit = dispatch.gas_limit();
            let dispatch = dispatch.into_stored();

            log::debug!(
                "Sending message {:?} from {:?} with gas limit {:?}",
                dispatch.message(),
                message_id,
                gas_limit,
            );

            if dispatch.value() != 0 {
                CurrencyOf::<T>::reserve(
                    &<T::AccountId as Origin>::from_origin(dispatch.source().into_origin()),
                    dispatch.value().unique_saturated_into(),
                ).unwrap_or_else(|_| unreachable!("Value reservation can't fail due to value sending rules. For more info, see module docs."));
            }

            if let Some(gas_limit) = gas_limit {
                // # Safety
                //
                // 1. There is no logic splitting value from the reserved nodes.
                // 2. The `gas_limit` has been checked inside message queue processing.
                // 3. The `value` of the value node has been checked before.
                // 4. The `dispatch.id()` is new generated by system from a checked
                //    ( inside message queue processing ) `message_id`.
                GasHandlerOf::<T>::split_with_value(message_id, dispatch.id(), gas_limit)
                    .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
            } else {
                // # Safety
                //
                // 1. There is no logic splitting value from the reserved nodes.
                // 2. The `dispatch.id()` is new generated by system from a checked
                //    ( inside message queue processing ) `message_id`.
                GasHandlerOf::<T>::split(message_id, dispatch.id())
                    .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
            }

            QueueOf::<T>::queue(dispatch)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
        } else {
            log::debug!(
                "Sending user message {:?} from {:?} with gas limit {:?}",
                dispatch.message(),
                message_id,
                dispatch.gas_limit(),
            );
            Pallet::<T>::send_user_message(message_id, dispatch.into_parts().1);
        }
    }

    fn wait_dispatch(
        &mut self,
        dispatch: StoredDispatch,
        duration: Option<u32>,
        reincarnation: bool,
    ) {
        Pallet::<T>::wait_dispatch(
            dispatch,
            duration.map(UniqueSaturatedInto::unique_saturated_into),
            if reincarnation {
                MessageWaitedRuntimeReason::WaitForCalled
            } else {
                MessageWaitedRuntimeReason::WaitCalled
            }
            .into_reason(),
        )
    }

    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
        delay: u32,
    ) {
        if delay.is_zero() {
            if let Some(dispatch) = Pallet::<T>::wake_dispatch(
                program_id,
                awakening_id,
                MessageWokenRuntimeReason::WakeCalled.into_reason(),
            ) {
                QueueOf::<T>::queue(dispatch)
                    .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));

                return;
            }
        } else if WaitlistOf::<T>::contains(&program_id, &awakening_id) {
            let expected_bn =
                SystemPallet::<T>::block_number().saturating_add(delay.unique_saturated_into());
            let task = ScheduledTask::WakeMessage(program_id, awakening_id);

            // This validation helps us to avoid returning error on insertion into `TaskPool` in case of duplicate wake.
            if !TaskPoolOf::<T>::contains(&expected_bn, &task) {
                TaskPoolOf::<T>::add(expected_bn, task)
                    .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));
            }

            return;
        }

        log::debug!(
            "Attempt to wake unknown message {:?} from {:?}",
            awakening_id,
            message_id
        );
    }

    fn update_pages_data(
        &mut self,
        program_id: ProgramId,
        pages_data: BTreeMap<PageNumber, PageBuf>,
    ) {
        self.state_changes.insert(program_id);
        let program_id = program_id.into_origin();
        let program = common::get_program(program_id)
            .expect("page update guaranteed to be called only for existing and active program");
        if let Program::Active(mut program) = program {
            for (page, data) in pages_data {
                common::set_program_page_data(program_id, page, data);
                program.pages_with_data.insert(page);
            }
            common::set_program(program_id, program);
        }
    }

    fn update_allocations(
        &mut self,
        program_id: ProgramId,
        allocations: BTreeSet<gear_core::memory::WasmPageNumber>,
    ) {
        let program_id = program_id.into_origin();
        let program = common::get_program(program_id)
            .expect("page update guaranteed to be called only for existing and active program");
        if let Program::Active(mut program) = program {
            let removed_pages = program.allocations.difference(&allocations);
            for page in removed_pages.flat_map(|p| p.to_gear_pages_iter()) {
                if program.pages_with_data.remove(&page) {
                    common::remove_program_page_data(program_id, page);
                }
            }
            program.allocations = allocations;
            common::set_program(program_id, program);
        }
    }

    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128) {
        let to = Pallet::<T>::inheritor_for(to.unwrap_or(from));
        let to = <T::AccountId as Origin>::from_origin(to.into_origin());
        let from = <T::AccountId as Origin>::from_origin(from.into_origin());
        let value = value.unique_saturated_into();

        Pallet::<T>::transfer_reserved(&from, &to, value);
    }

    fn store_new_programs(&mut self, code_id: CodeId, candidates: Vec<(ProgramId, MessageId)>) {
        if T::CodeStorage::get_code(code_id).is_some() {
            for (candidate_id, init_message) in candidates {
                if !GearProgramPallet::<T>::program_exists(candidate_id) {
                    self.set_program(candidate_id, code_id, init_message);
                } else {
                    log::debug!("Program with id {:?} already exists", candidate_id);
                }
            }
        } else {
            log::debug!(
                "No referencing code with code hash {:?} for candidate programs",
                code_id
            );
            for (candidate, _) in candidates {
                self.programs.insert(candidate);
            }
        }
    }

    fn stop_processing(&mut self, dispatch: StoredDispatch, gas_burned: u64) {
        log::debug!(
            "Not enough gas for processing msg id {}, allowance equals {}, gas tried to burn at least {}",
            dispatch.id(),
            GasAllowanceOf::<T>::get(),
            gas_burned,
        );

        SentOf::<T>::increase();
        GasAllowanceOf::<T>::decrease(gas_burned);
        QueueOf::<T>::requeue(dispatch)
            .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
    }
}
