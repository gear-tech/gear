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
    internal::HoldBound,
    manager::{CodeInfo, ExtManager},
    Config, CostsPerBlockOf, CurrencyOf, Event, GasAllowanceOf, GasHandlerOf, Pallet,
    ProgramStorageOf, QueueOf, SentOf, TaskPoolOf, WaitlistOf,
};
use common::{
    event::*,
    scheduler::{ScheduledTask, SchedulingCostsPerBlock, TaskHandler, TaskPool},
    storage::*,
    CodeStorage, GasTree, Origin, Program, ProgramState, ProgramStorage,
};
use core_processor::common::{DispatchOutcome as CoreDispatchOutcome, JournalHandler};
use frame_support::{
    sp_runtime::Saturating,
    traits::{Currency, ExistenceRequirement, ReservableCurrency},
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::{
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::{GearPage, PageBuf, PageU32Size},
    message::{Dispatch, MessageWaitedType, StoredDispatch},
    reservation::GasReserver,
};
use gear_core_errors::SimpleSignalError;
use sp_runtime::traits::{UniqueSaturatedInto, Zero};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    prelude::*,
};

impl<T> JournalHandler for ExtManager<T>
where
    T: Config,
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
            ProgramStorageOf::<T>::waiting_init_take_messages(p_id)
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
                ProgramStorageOf::<T>::update_active_program(program_id, |p, bn_ref| {
                    *bn_ref = Pallet::<T>::block_number();
                    p.state = ProgramState::Initialized;
                })
                .unwrap_or_else(|e| {
                    unreachable!(
                        "Program initialized status may only be set to active program {:?}",
                        e
                    );
                });

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
                program_id,
                origin,
                executed,
                ..
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

                // If we run into `InitFailure` after real execution (not
                // prepare or precharge) processor methods, then we are
                // sure that it was active program.
                let maybe_inactive = !executed;

                self.clean_reservation_tasks(program_id, maybe_inactive);

                ProgramStorageOf::<T>::update_program_if_active(program_id, |p, bn_ref| {
                    *bn_ref = Pallet::<T>::block_number();
                    *p = Program::Terminated(origin);
                }).unwrap_or_else(|e| {
                    if !maybe_inactive {
                        unreachable!(
                            "Program terminated status may only be set to an existing active program: {:?}",
                            e,
                        );
                    }
                });

                ProgramStorageOf::<T>::remove_program_pages(program_id);

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
            NoExecution => {
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
        log::debug!("Exit dispatch");

        let reason = MessageWokenSystemReason::ProgramGotInitialized.into_reason();

        WaitlistOf::<T>::drain_key(id_exited).for_each(|entry| {
            let message = Pallet::<T>::wake_dispatch_requirements(entry, reason.clone());

            QueueOf::<T>::queue(message)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
        });

        let _ = ProgramStorageOf::<T>::waiting_init_take_messages(id_exited);

        // Program can't be inactive, cause it was executed.
        self.clean_reservation_tasks(id_exited, false);

        ProgramStorageOf::<T>::update_program_if_active(id_exited, |p, bn_ref| {
            *bn_ref = Pallet::<T>::block_number();
            *p = Program::Exited(value_destination);
        })
        .unwrap_or_else(|e| {
            unreachable!("`exit` can be called only from active program: {:?}", e);
        });

        ProgramStorageOf::<T>::remove_program_pages(id_exited);

        let program_account = &<T::AccountId as Origin>::from_origin(id_exited.into_origin());
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
        Pallet::<T>::consume_and_retrieve(message_id)
    }

    fn send_dispatch(
        &mut self,
        message_id: MessageId,
        dispatch: Dispatch,
        delay: u32,
        reservation: Option<ReservationId>,
    ) {
        // This method shouldn't reduce gas allowance for enqueueing dispatch,
        // because message already charged for it within the env.

        let to_user = self.check_user_id(&dispatch.destination());

        if !delay.is_zero() {
            log::debug!("Sending delayed for {delay} blocks dispatch");
            Pallet::<T>::send_delayed_dispatch(message_id, dispatch, delay, to_user, reservation)
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

            match (gas_limit, reservation) {
                (Some(gas_limit), None) => {
                    // # Safety
                    //
                    // 1. There is no logic splitting value from the reserved nodes.
                    // 2. The `gas_limit` has been checked inside message queue processing.
                    // 3. The `value` of the value node has been checked before.
                    // 4. The `dispatch.id()` is new generated by system from a checked
                    //    ( inside message queue processing ) `message_id`.
                    GasHandlerOf::<T>::split_with_value(message_id, dispatch.id(), gas_limit)
                        .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
                }
                (None, None) => {
                    // # Safety
                    //
                    // 1. There is no logic splitting value from the reserved nodes.
                    // 2. The `dispatch.id()` is new generated by system from a checked
                    //    ( inside message queue processing ) `message_id`.
                    GasHandlerOf::<T>::split(message_id, dispatch.id())
                        .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
                }
                (Some(_gas_limit), Some(_reservation_id)) => {
                    // TODO: #1828
                    unreachable!(
                        "Sending dispatch with gas limit from reservation \
                    is currently unimplemented and there is no way to send such dispatch"
                    );
                }
                (None, Some(reservation_id)) => {
                    GasHandlerOf::<T>::split(reservation_id, dispatch.id())
                        .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

                    Pallet::<T>::remove_gas_reservation_with_task(
                        dispatch.source(),
                        reservation_id,
                    );
                }
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
            Pallet::<T>::send_user_message(message_id, dispatch.into_parts().1, reservation);
        }
    }

    fn wait_dispatch(
        &mut self,
        dispatch: StoredDispatch,
        duration: Option<u32>,
        waited_type: MessageWaitedType,
    ) {
        // This method shouldn't reduce gas allowance for waiting dispatch,
        // because message already charged for it within the env.
        Pallet::<T>::wait_dispatch(
            dispatch,
            duration.map(UniqueSaturatedInto::unique_saturated_into),
            MessageWaitedRuntimeReason::from(waited_type).into_reason(),
        )
    }

    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
        delay: u32,
    ) {
        // This method shouldn't reduce gas allowance for waking dispatch,
        // because message already charged for it within the env.

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
                Pallet::<T>::block_number().saturating_add(delay.unique_saturated_into());
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
        pages_data: BTreeMap<GearPage, PageBuf>,
    ) {
        self.state_changes.insert(program_id);

        ProgramStorageOf::<T>::update_active_program(program_id, |p, _bn| {
            for (page, data) in pages_data {
                ProgramStorageOf::<T>::set_program_page_data(program_id, page, data);
                p.pages_with_data.insert(page);
            }
        })
        .unwrap_or_else(|e| {
            unreachable!(
                "Page update guaranteed to be called only for existing and active program: {:?}",
                e
            )
        });
    }

    fn update_allocations(
        &mut self,
        program_id: ProgramId,
        allocations: BTreeSet<gear_core::memory::WasmPage>,
    ) {
        ProgramStorageOf::<T>::update_active_program(program_id, |p, _bn| {
            let removed_pages = p.allocations.difference(&allocations);
            for page in removed_pages.flat_map(|page| page.to_pages_iter()) {
                if p.pages_with_data.remove(&page) {
                    ProgramStorageOf::<T>::remove_program_page_data(program_id, page);
                }
            }

            p.allocations = allocations;
        }).unwrap_or_else(|e| {
            unreachable!("Allocations update guaranteed to be called only for existing and active program: {:?}", e)
        });
    }

    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128) {
        let to = Pallet::<T>::inheritor_for(to.unwrap_or(from));
        let to = <T::AccountId as Origin>::from_origin(to.into_origin());
        let from = <T::AccountId as Origin>::from_origin(from.into_origin());
        let value = value.unique_saturated_into();

        Pallet::<T>::transfer_reserved(&from, &to, value);
    }

    fn store_new_programs(&mut self, code_id: CodeId, candidates: Vec<(MessageId, ProgramId)>) {
        if let Some(code) = T::CodeStorage::get_code(code_id) {
            let code_info = CodeInfo::from_code(&code_id, &code);
            for (init_message, candidate_id) in candidates {
                if !ProgramStorageOf::<T>::program_exists(candidate_id) {
                    let block_number = Pallet::<T>::block_number();
                    self.set_program(candidate_id, &code_info, init_message, block_number);
                } else {
                    log::debug!("Program with id {:?} already exists", candidate_id);
                }
            }
        } else {
            log::debug!(
                "No referencing code with code hash {:?} for candidate programs",
                code_id
            );
            for (_, candidate) in candidates {
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

    fn reserve_gas(
        &mut self,
        message_id: MessageId,
        reservation_id: ReservationId,
        program_id: ProgramId,
        amount: u64,
        duration: u32,
    ) {
        log::debug!(
            "Reserved: {:?} from {:?} with {:?} for {} blocks",
            amount,
            message_id,
            reservation_id,
            duration
        );

        let hold = HoldBound::<T>::by(CostsPerBlockOf::<T>::reservation())
            .duration(BlockNumberFor::<T>::from(duration));

        // Validating holding duration.
        if hold.expected_duration().is_zero() {
            unreachable!("Threshold for reservation invalidated")
        }

        let total_amount = amount.saturating_add(hold.lock());

        GasHandlerOf::<T>::reserve(message_id, reservation_id, total_amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted: {:?}", e));

        GasHandlerOf::<T>::lock(reservation_id, hold.lock())
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        TaskPoolOf::<T>::add(
            hold.expected(),
            ScheduledTask::RemoveGasReservation(program_id, reservation_id),
        )
        .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));
    }

    fn unreserve_gas(
        &mut self,
        reservation_id: ReservationId,
        program_id: ProgramId,
        expiration: u32,
    ) {
        <Self as TaskHandler<T::AccountId>>::remove_gas_reservation(
            self,
            program_id,
            reservation_id,
        );

        let _ = TaskPoolOf::<T>::delete(
            BlockNumberFor::<T>::from(expiration),
            ScheduledTask::RemoveGasReservation(program_id, reservation_id),
        );
    }

    fn update_gas_reservation(&mut self, program_id: ProgramId, reserver: GasReserver) {
        ProgramStorageOf::<T>::update_active_program(program_id, |p, _bn| {
            p.gas_reservation_map = reserver.into_map(
                Pallet::<T>::block_number().unique_saturated_into(),
                |duration| {
                    HoldBound::<T>::by(CostsPerBlockOf::<T>::reservation())
                        .duration(BlockNumberFor::<T>::from(duration))
                        .expected()
                        .unique_saturated_into()
                },
            );
        })
        .unwrap_or_else(|e| {
            unreachable!(
                "Gas reservation update guaranteed to be called only on an existing program: {:?}",
                e
            )
        });
    }

    fn system_reserve_gas(&mut self, message_id: MessageId, amount: u64) {
        log::debug!("Reserve {} of gas for system from {}", amount, message_id);

        GasHandlerOf::<T>::system_reserve(message_id, amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
    }

    fn system_unreserve_gas(&mut self, message_id: MessageId) {
        let amount = GasHandlerOf::<T>::system_unreserve(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        if amount != 0 {
            log::debug!("Unreserved {} gas for system from {}", amount, message_id);
        } else {
            log::debug!(
                "Gas for system was not unreserved from {} as there is no supply",
                message_id
            );
        }
    }

    fn send_signal(
        &mut self,
        message_id: MessageId,
        destination: ProgramId,
        err: SimpleSignalError,
    ) {
        ExtManager::send_signal(self, message_id, destination, err)
    }
}
