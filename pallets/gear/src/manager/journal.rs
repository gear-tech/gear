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

use crate::{
    Config, CostsPerBlockOf, CurrencyOf, EXISTENTIAL_DEPOSIT_LOCK_ID, Event, GasAllowanceOf,
    GasHandlerOf, GasTree, GearBank, Pallet, ProgramStorageOf, QueueOf, TaskPoolOf, WaitlistOf,
    internal::HoldBoundBuilder, manager::ExtManager,
};
use alloc::format;
use common::{
    CodeStorage, LockableTree, Origin, ProgramStorage, ReservableTree,
    event::*,
    scheduler::{SchedulingCostsPerBlock, StorageType, TaskPool},
    storage::*,
};
use core_processor::common::{DispatchOutcome as CoreDispatchOutcome, JournalHandler};
use frame_support::{
    sp_runtime::Saturating,
    traits::{Currency, ExistenceRequirement, LockableCurrency, WithdrawReasons},
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::{
    env::MessageWaitedType,
    ids::{ActorId, CodeId, MessageId, ReservationId},
    memory::PageBuf,
    message::{Dispatch, StoredDispatch},
    pages::{GearPage, WasmPage, numerated::tree::IntervalsTree},
    program::{Program, ProgramState},
    reservation::GasReserver,
    tasks::{ScheduledTask, TaskHandler},
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
        source: ActorId,
        outcome: CoreDispatchOutcome,
    ) {
        use CoreDispatchOutcome::*;

        let status = match outcome {
            Exit { program_id } => {
                log::trace!("Dispatch outcome exit: {message_id:?}");

                Pallet::<T>::deposit_event(Event::ProgramChanged {
                    id: program_id,
                    change: ProgramChangeKind::Inactive,
                });

                DispatchStatus::Success
            }
            Success => {
                log::trace!("Dispatch outcome success: {message_id:?}");

                DispatchStatus::Success
            }
            MessageTrap { program_id, trap } => {
                log::trace!("Dispatch outcome trap: {message_id:?}");
                log::debug!(
                    "🪤 Program {} terminated with a trap: {}",
                    program_id.into_origin(),
                    trap
                );

                DispatchStatus::Failed
            }
            InitSuccess { program_id, .. } => {
                log::trace!("Dispatch ({message_id:?}) init success for program {program_id:?}");

                let expiration =
                    ProgramStorageOf::<T>::update_program_if_active(program_id, |p, bn| {
                        match p {
                            Program::Active(active) => active.state = ProgramState::Initialized,
                            actual_program => {
                                // Guaranteed to be called on existing program, because only existing programs
                                // are able to be initialized.
                                let err_msg = format!("JournalHandler::message_dispatched: failed to update active program state. \
                                Program - {program_id}, actual program - {actual_program:?}");

                                log::error!("{err_msg}");
                                unreachable!("{err_msg}")
                            }
                        }

                        bn
                    })
                    .unwrap_or_else(|e| {
                        // Guaranteed to be called on existing program
                        let err_msg = format!("JournalHandler::message_dispatched: failed to update program. \
                        Program - {program_id}. Got error: {e:?}");

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}")
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
                log::trace!("Dispatch ({message_id:?}) for program wasn't executed");

                DispatchStatus::NotExecuted
            }
        };

        if self.check_user_id(&source) {
            self.dispatch_statuses.insert(message_id, status);
        }
    }

    fn gas_burned(&mut self, message_id: MessageId, amount: u64) {
        log::debug!("Burned: {amount:?} from: {message_id:?}");

        GasAllowanceOf::<T>::decrease(amount);

        Pallet::<T>::spend_burned(message_id, amount)
    }

    fn exit_dispatch(&mut self, id_exited: ActorId, value_destination: ActorId) {
        log::debug!(
            "Exit dispatch: id_exited = {id_exited}, value_destination = {value_destination}"
        );

        Self::clean_waitlist(id_exited);

        ProgramStorageOf::<T>::update_program_if_active(id_exited, |p, bn| {
            let _ = TaskPoolOf::<T>::delete(bn, ScheduledTask::PauseProgram(id_exited));

            match p {
                Program::Active(program) => {
                    Self::clean_inactive_program(id_exited, program, value_destination)
                }
                actual_program => {
                    // Guaranteed to be called only on active program
                    let err_msg = format!(
                        "JournalHandler::exit_dispatch: failed to exit active program. \
                    Program - {id_exited}, actual program - {actual_program:?}"
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}")
                }
            }

            *p = Program::Exited(value_destination);
        })
        .unwrap_or_else(|e| {
            // Guaranteed to be called only on active program
            let err_msg = format!(
                "ExtManager::exit_dispatch: failed to update program. \
            Program - {id_exited}. Got error: {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
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

            // It's necessary to deposit value so the source would have enough
            // balance locked (in gear-bank) for future value processing.
            //
            // In case of error replies, we don't need to do it, since original
            // message value is already on locked balance in gear-bank.
            if dispatch.value() != 0 && !dispatch.is_error_reply() {
                GearBank::<T>::deposit_value(
                    &dispatch.source().cast(),
                    dispatch.value().unique_saturated_into(),
                    false,
                )
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "JournalHandler::send_dispatch: failed depositing value on gear bank. \
                        Sender - {sender}, value - {value}. Got error - {e:?}",
                        sender = dispatch.source(),
                        value = dispatch.value(),
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
            }

            match (gas_limit, reservation) {
                (Some(gas_limit), None) => Pallet::<T>::split_with_value(
                    message_id,
                    dispatch.id(),
                    gas_limit,
                    dispatch.is_reply(),
                ),
                (None, None) => Pallet::<T>::split(message_id, dispatch.id(), dispatch.is_reply()),
                (Some(gas_limit), Some(reservation_id)) => {
                    // TODO: #1828
                    let err_msg = format!(
                        "JournalHandler::send_dispatch: sending dispatch with gas from reservation isn't implemented. \
                        Message - {message_id}, sender - {sender}, gas limit - {gas_limit}, reservation - {reservation_id}",
                        sender = dispatch.source(),
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                }
                (None, Some(reservation_id)) => {
                    Pallet::<T>::split(reservation_id, dispatch.id(), dispatch.is_reply());
                    Pallet::<T>::remove_gas_reservation_with_task(
                        dispatch.source(),
                        reservation_id,
                    );
                }
            }

            QueueOf::<T>::queue(dispatch).unwrap_or_else(|e| {
                let err_msg = format!(
                    "JournalHandler::send_dispatch: failed queuing message. Got error - {e:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });
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
        program_id: ActorId,
        awakening_id: MessageId,
        delay: u32,
    ) {
        // This method shouldn't reduce gas allowance for waking dispatch,
        // because message already charged for it within the env.

        if delay.is_zero() {
            if let Ok(dispatch) = Pallet::<T>::wake_dispatch(
                program_id,
                awakening_id,
                MessageWokenRuntimeReason::WakeCalled.into_reason(),
            ) {
                QueueOf::<T>::queue(dispatch).unwrap_or_else(|e| {
                    let err_msg = format!(
                        "JournalHandler::wake_message: failed queuing message. Got error - {e:?}"
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });

                return;
            }
        } else if WaitlistOf::<T>::contains(&program_id, &awakening_id) {
            let expected_bn =
                Pallet::<T>::block_number().saturating_add(delay.unique_saturated_into());
            let task = ScheduledTask::WakeMessage(program_id, awakening_id);

            // This validation helps us to avoid returning error on insertion into `TaskPool` in case of duplicate wake.
            if !TaskPoolOf::<T>::contains(&expected_bn, &task) {
                TaskPoolOf::<T>::add(expected_bn, task).unwrap_or_else(|e| {
                    let err_msg = format!(
                        "JournalHandler::wake_message: failed adding task for waking message. \
                        Expected bn - {expected_bn:?}, program id - {program_id}, message_id - {awakening_id}.
                        Got error - {e:?}"
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
            }

            return;
        }

        log::debug!("Attempt to wake unknown message {awakening_id:?} from {message_id:?}");
    }

    fn update_pages_data(&mut self, program_id: ActorId, pages_data: BTreeMap<GearPage, PageBuf>) {
        self.state_changes.insert(program_id);

        // TODO: pass `memory_infix` as argument #4025
        let memory_infix = ProgramStorageOf::<T>::memory_infix(program_id).unwrap_or_else(|| {
            // Guaranteed to be called on existing active program
            let err_msg =
                format!("JournalHandler::update_pages_data: program is not active {program_id}");

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });

        for (page, data) in pages_data {
            ProgramStorageOf::<T>::set_program_page_data(program_id, memory_infix, page, data);
        }
    }

    fn update_allocations(&mut self, program_id: ActorId, allocations: IntervalsTree<WasmPage>) {
        // TODO: pass `memory_infix` as argument #4025
        let memory_infix = ProgramStorageOf::<T>::memory_infix(program_id).unwrap_or_else(|| {
            // Guaranteed to be called on existing active program
            let err_msg =
                format!("JournalHandler::update_allocations: program is not active {program_id}.");

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });

        let old_allocations = ProgramStorageOf::<T>::allocations(program_id).unwrap_or_default();
        let remove_pages = old_allocations
            .difference(&allocations)
            .flat_map(|i| i.iter())
            .flat_map(|i| i.to_iter());
        ProgramStorageOf::<T>::remove_data_for_pages(program_id, memory_infix, remove_pages);
        ProgramStorageOf::<T>::set_allocations(program_id, allocations.clone());
    }

    fn send_value(&mut self, from: ActorId, to: ActorId, value: u128, locked: bool) {
        let from = from.cast();
        let to = to.cast();
        let value = value.unique_saturated_into();

        if locked {
            GearBank::<T>::transfer_locked_value(&from, &to, value).unwrap_or_else(|e| {
                let err_msg = format!(
                    "JournalHandler::send_value: failed transferring bank locked value. \
                    From - {from:?}, to - {to:?}, value - {value:?}. Got error: {e:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });
        } else {
            GearBank::<T>::transfer_value(&from, &to, value).unwrap_or_else(|e| {
                let err_msg = format!(
                    "JournalHandler::send_value: failed transferring bank value. \
                    From - {from:?}, to - {to:?}, value - {value:?}. Got error: {e:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });
        }
    }

    fn store_new_programs(
        &mut self,
        program_id: ActorId,
        code_id: CodeId,
        candidates: Vec<(MessageId, ActorId)>,
    ) {
        if T::CodeStorage::original_code_exists(code_id) {
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
                    .unwrap_or_else(|e| {
                        let err_msg = format!(
                            "JournalHandler::store_new_programs: failed transferring ED to a new program. \
                            Sender - {program_id}, dest - {candidate_id}, value - {ed:?}. Got error - {e:?}"
                        );

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}");
                    });
                    // Set lock to avoid accidental account removal by the runtime.
                    CurrencyOf::<T>::set_lock(
                        EXISTENTIAL_DEPOSIT_LOCK_ID,
                        &candidate_account,
                        ed,
                        WithdrawReasons::all(),
                    );

                    self.set_program(candidate_id, code_id, init_message, block_number);

                    Pallet::<T>::deposit_event(Event::ProgramChanged {
                        id: candidate_id,
                        change: ProgramChangeKind::ProgramSet {
                            expiration: block_number,
                        },
                    });
                } else {
                    log::debug!("Program with id {candidate_id:?} already exists");
                }
            }
        } else {
            log::debug!("No referencing code with code hash {code_id:?} for candidate programs");
            // SAFETY:
            // Do not remove insertion into programs map as it gives guarantee
            // that init message for destination with no code won't enter
            // the mailbox (so no possible uncovered gas charges which leads to panic).
            // Such message will be inserted into the queue and later processed as
            // non executable.
            //
            // Test for it - `test_create_program_no_code_hash`.
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
        // TODO: #3112. Rework requeuing logic to avoid blocked queue.
        QueueOf::<T>::requeue(dispatch).unwrap_or_else(|e| {
            let err_msg = format!(
                "JournalHandler::stop_processing: failed requeuing message. Got error - {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });
    }

    fn reserve_gas(
        &mut self,
        message_id: MessageId,
        reservation_id: ReservationId,
        program_id: ActorId,
        amount: u64,
        duration: u32,
    ) {
        log::debug!(
            "Reserved: {amount:?} from {message_id:?} with {reservation_id:?} for {duration} blocks"
        );

        let hold = HoldBoundBuilder::<T>::new(StorageType::Reservation)
            .duration(BlockNumberFor::<T>::from(duration));

        // Validating holding duration.
        if hold.expected_duration().is_zero() {
            let err_msg = format!(
                "JournalHandler::reserve_gas: reservation got zero duration hold bound for storing. \
                Duration - {duration}, block cost - {cost}, program - {program_id}.",
                cost = CostsPerBlockOf::<T>::by_storage_type(StorageType::Reservation)
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        }

        let total_amount = amount.saturating_add(hold.lock_amount());

        GasHandlerOf::<T>::reserve(message_id, reservation_id, total_amount).unwrap_or_else(|e| {
            let err_msg = format!(
                "JournalHandler::reserve_gas: failed reserving gas. Origin message id - {message_id}, \
                reservation id - {reservation_id}, reservation amount - {amount}, hold lock - {lock}. \
                Got error - {e:?}",
                lock = hold.lock_amount(),
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });

        let lock_id = hold.lock_id().unwrap_or_else(|| {
            // Reservation storage is guaranteed to have an associated lock id
            let err_msg =
                "JournalHandler::reserve_gas: No associated lock id for the reservation storage";

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });
        GasHandlerOf::<T>::lock(reservation_id, lock_id, hold.lock_amount()).unwrap_or_else(|e| {
            let err_msg = format!(
                "JournalHandler::reserve_gas: failed locking gas for the reservation hold. \
                Reseravation - {reservation_id}, lock amount - {lock}. Got error - {e:?}",
                lock = hold.lock_amount()
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });

        TaskPoolOf::<T>::add(
            hold.expected(),
            ScheduledTask::RemoveGasReservation(program_id, reservation_id),
        )
        .unwrap_or_else(|e| {
            let err_msg = format!(
                "JournalHandler::reserve_gas: failed adding task for gas reservation removal. \
                Expected bn - {bn:?}, program id - {program_id}, reservation id - {reservation_id}. Got error - {e:?}",
                bn = hold.expected()
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });
    }

    fn unreserve_gas(
        &mut self,
        reservation_id: ReservationId,
        program_id: ActorId,
        expiration: u32,
    ) {
        <Self as TaskHandler<T::AccountId, MessageId, bool>>::remove_gas_reservation(
            self,
            program_id,
            reservation_id,
        );

        let _ = TaskPoolOf::<T>::delete(
            BlockNumberFor::<T>::from(expiration),
            ScheduledTask::RemoveGasReservation(program_id, reservation_id),
        );
    }

    fn update_gas_reservation(&mut self, program_id: ActorId, reserver: GasReserver) {
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
            // Guaranteed to be called on existing active program
            let err_msg = format!(
                "JournalHandler::update_gas_reservation: failed to update program. \
            Program - {program_id}. Got error: {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });
    }

    fn system_reserve_gas(&mut self, message_id: MessageId, amount: u64) {
        log::debug!("Reserve {amount} of gas for system from {message_id}");

        GasHandlerOf::<T>::system_reserve(message_id, amount).unwrap_or_else(|e| {
            let err_msg = format!(
                "JournalHandler::system_reserve_gas: failed system reserve gas. \
            Message id - {message_id}, amount - {amount}. Got error: {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });
    }

    fn system_unreserve_gas(&mut self, message_id: MessageId) {
        let amount = GasHandlerOf::<T>::system_unreserve(message_id).unwrap_or_else(|e| {
            let err_msg = format!(
                "JournalHandler::system_unreserve_gas: failed system unreserve. \
                Message id - {message_id}. Got error: {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });

        if amount != 0 {
            log::debug!("Unreserved {amount} gas for system from {message_id}");
        } else {
            log::debug!(
                "Gas for system was not unreserved from {message_id} as there is no supply"
            );
        }
    }

    fn send_signal(&mut self, message_id: MessageId, destination: ActorId, code: SignalCode) {
        Self::send_signal(self, message_id, destination, code)
    }

    fn reply_deposit(&mut self, message_id: MessageId, future_reply_id: MessageId, amount: u64) {
        log::debug!("Creating reply deposit {amount} gas for message id {future_reply_id}");

        GasHandlerOf::<T>::create_deposit(message_id, future_reply_id, amount).unwrap_or_else(|e| {
            let err_msg = format!(
                "JournalHandler::reply_deposit: failed creating reply deposit. Message id - {message_id}, \
                future reply id - {future_reply_id}, amount - {amount}. Got error - {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });
    }
}
