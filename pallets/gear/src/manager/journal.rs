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

use crate::{
    internal::HoldBoundBuilder,
    manager::{CodeInfo, ExtManager},
    Config, CurrencyOf, Event, GasAllowanceOf, GasHandlerOf, GasTree, GearBank, Pallet,
    ProgramStorageOf, QueueOf, TaskPoolOf, WaitlistOf, EXISTENTIAL_DEPOSIT_LOCK_ID,
};
use common::{
    event::*,
    scheduler::{ScheduledTask, StorageType, TaskHandler, TaskPool},
    storage::*,
    CodeStorage, LockableTree, Origin, ProgramStorage, ReservableTree,
};
use core_processor::common::{DispatchOutcome as CoreDispatchOutcome, JournalHandler};
use frame_support::{
    sp_runtime::Saturating,
    traits::{Currency, ExistenceRequirement, LockableCurrency, WithdrawReasons},
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::{
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::PageBuf,
    message::{Dispatch, MessageWaitedType, StoredDispatch},
    pages::{numerated::tree::IntervalsTree, GearPage, WasmPage},
    program::{Program, ProgramState},
    reservation::GasReserver,
};
use gear_core_errors::SignalCode;
use sp_runtime::traits::{UniqueSaturatedInto, Zero};
use sp_std::{collections::btree_map::BTreeMap, prelude::*};

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

                let expiration =
                    ProgramStorageOf::<T>::update_program_if_active(program_id, |p, bn| {
                        match p {
                            Program::Active(active) => active.state = ProgramState::Initialized,
                            _ => unreachable!("Only active programs are able to initialize"),
                        }

                        bn
                    })
                    .unwrap_or_else(|e| {
                        unreachable!(
                            "Program initialized status may only be set to active program {:?}",
                            e
                        );
                    });

                Pallet::<T>::deposit_event(Event::ProgramChanged {
                    id: program_id,
                    change: ProgramChangeKind::Active { expiration },
                });

                DispatchStatus::Success
            }
            InitFailure {
                program_id,
                origin,
                reason,
            } => {
                log::trace!(
                    "Dispatch ({message_id:?}) init failure for program {program_id:?}: {reason}"
                );

                Self::process_failed_init(program_id, origin);

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

        Pallet::<T>::spend_burned(message_id, amount)
    }

    fn exit_dispatch(&mut self, id_exited: ProgramId, value_destination: ProgramId) {
        log::debug!(
            "Exit dispatch: id_exited = {id_exited}, value_destination = {value_destination}"
        );

        Self::clean_waitlist(id_exited);

        ProgramStorageOf::<T>::update_program_if_active(id_exited, |p, bn| {
            let _ = TaskPoolOf::<T>::delete(bn, ScheduledTask::PauseProgram(id_exited));

            match p {
                Program::Active(program) => {
                    Self::remove_gas_reservation_map(
                        id_exited,
                        core::mem::take(&mut program.gas_reservation_map),
                    );

                    Self::clean_inactive_program(
                        id_exited,
                        program.memory_infix,
                        value_destination,
                    );
                }
                _ => unreachable!("Action executed only for active program"),
            }

            *p = Program::Exited(value_destination);
        })
        .unwrap_or_else(|e| {
            unreachable!("`exit` can be called only from active program: {:?}", e);
        });
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
                GearBank::<T>::deposit_value(
                    &dispatch.source().cast(),
                    dispatch.value().unique_saturated_into(),
                    false,
                )
                .unwrap_or_else(|e| unreachable!("Gear bank error: {e:?}"));
            }

            match (gas_limit, reservation) {
                (Some(gas_limit), None) => Pallet::<T>::split_with_value(
                    message_id,
                    dispatch.id(),
                    gas_limit,
                    dispatch.is_reply(),
                ),
                (None, None) => Pallet::<T>::split(message_id, dispatch.id(), dispatch.is_reply()),
                (Some(_gas_limit), Some(_reservation_id)) => {
                    // TODO: #1828
                    unreachable!(
                        "Sending dispatch with gas limit from reservation \
                    is currently unimplemented and there is no way to send such dispatch"
                    );
                }
                (None, Some(reservation_id)) => {
                    Pallet::<T>::split(reservation_id, dispatch.id(), dispatch.is_reply());
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

        // TODO: pass `memory_infix` as argument #4025
        let memory_infix = ProgramStorageOf::<T>::memory_infix(program_id).unwrap_or_else(|| {
            unreachable!(
                "Program with id {:?} is guaranteed to be active, when updating pages data",
                program_id
            )
        });

        ProgramStorageOf::<T>::append_pages_with_data(program_id, pages_data.keys().copied());
        for (page, data) in pages_data {
            ProgramStorageOf::<T>::set_program_page_data(program_id, memory_infix, page, data);
        }
    }

    fn update_allocations(&mut self, program_id: ProgramId, allocations: IntervalsTree<WasmPage>) {
        // TODO: pass `memory_infix` as argument #4025
        let memory_infix = ProgramStorageOf::<T>::memory_infix(program_id).unwrap_or_else(|| {
            unreachable!(
                "Program with id {:?} is guaranteed to be active, when updating pages data",
                program_id
            )
        });

        let old_allocations = ProgramStorageOf::<T>::allocations(program_id).unwrap_or_default();
        let remove_pages = old_allocations
            .difference(&allocations)
            .flat_map(|i| i.iter())
            .flat_map(|i| i.to_iter());
        ProgramStorageOf::<T>::remove_pages_with_data(program_id, memory_infix, remove_pages);
        ProgramStorageOf::<T>::set_allocations(program_id, allocations.clone());
    }

    fn send_value(&mut self, from: ProgramId, to: Option<ProgramId>, value: u128) {
        let to = Pallet::<T>::inheritor_for(to.unwrap_or(from)).cast();
        let from = from.cast();
        let value = value.unique_saturated_into();

        GearBank::<T>::transfer_value(&from, &to, value)
            .unwrap_or_else(|e| unreachable!("Gear bank error: {e:?}"));
    }

    fn store_new_programs(
        &mut self,
        program_id: ProgramId,
        code_id: CodeId,
        candidates: Vec<(MessageId, ProgramId)>,
    ) {
        if let Some(code) = T::CodeStorage::get_code(code_id) {
            let code_info = CodeInfo::from_code(&code_id, &code);
            for (init_message, candidate_id) in candidates {
                if !Pallet::<T>::program_exists(self.builtins(), candidate_id) {
                    let block_number = Pallet::<T>::block_number();

                    let candidate_account = candidate_id.cast();
                    let ed = CurrencyOf::<T>::minimum_balance();

                    // Make sure an account exists for the newly created program.
                    // Balance validity check has been performed so we don't expect any errors.
                    CurrencyOf::<T>::transfer(
                        &program_id.cast(),
                        &candidate_account,
                        ed,
                        ExistenceRequirement::KeepAlive,
                    )
                    .unwrap_or_else(|e| unreachable!("Existential deposit transfer error: {e:?}"));
                    // Set lock to avoid accidental account removal by the runtime.
                    CurrencyOf::<T>::set_lock(
                        EXISTENTIAL_DEPOSIT_LOCK_ID,
                        &candidate_account,
                        ed,
                        WithdrawReasons::all(),
                    );

                    self.set_program(candidate_id, &code_info, init_message, block_number);

                    Pallet::<T>::deposit_event(Event::ProgramChanged {
                        id: candidate_id,
                        change: ProgramChangeKind::ProgramSet {
                            expiration: block_number,
                        },
                    });
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

        GasAllowanceOf::<T>::decrease(gas_burned);
        // TODO: #3112. Rework requeueing logic to avoid blocked queue.
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

        let hold = HoldBoundBuilder::<T>::new(StorageType::Reservation)
            .duration(BlockNumberFor::<T>::from(duration));

        // Validating holding duration.
        if hold.expected_duration().is_zero() {
            unreachable!("Threshold for reservation invalidated")
        }

        let total_amount = amount.saturating_add(hold.lock_amount());

        GasHandlerOf::<T>::reserve(message_id, reservation_id, total_amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted: {:?}", e));

        let lock_id = hold.lock_id().unwrap_or_else(|| {
            unreachable!("Reservation storage is guaranteed to have an associated lock id")
        });
        GasHandlerOf::<T>::lock(reservation_id, lock_id, hold.lock_amount())
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
        ProgramStorageOf::<T>::update_active_program(program_id, |p| {
            p.gas_reservation_map = reserver.into_map(
                Pallet::<T>::block_number().unique_saturated_into(),
                |duration| {
                    HoldBoundBuilder::<T>::new(StorageType::Reservation)
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

    fn send_signal(&mut self, message_id: MessageId, destination: ProgramId, code: SignalCode) {
        Self::send_signal(self, message_id, destination, code)
    }

    fn reply_deposit(&mut self, message_id: MessageId, future_reply_id: MessageId, amount: u64) {
        log::debug!("Creating reply deposit {amount} gas for message id {future_reply_id}");

        GasHandlerOf::<T>::create_deposit(message_id, future_reply_id, amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
    }
}
