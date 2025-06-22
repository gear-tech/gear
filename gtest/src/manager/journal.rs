// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Implementation of the `JournalHandler` trait for the `ExtManager`.

use super::{ExtManager, Program, TestActor};
use crate::{
    manager::hold_bound::HoldBoundBuilder,
    program::ProgramBuilder,
    state::{accounts::Accounts, actors::Actors},
    Value, EXISTENTIAL_DEPOSIT,
};
use core_processor::common::{DispatchOutcome, JournalHandler};
use gear_common::{
    event::{MessageWaitedRuntimeReason, RuntimeReason},
    scheduler::StorageType,
};
use gear_core::{
    env::MessageWaitedType,
    ids::{ActorId, CodeId, MessageId, ReservationId},
    memory::PageBuf,
    message::{Dispatch, SignalMessage, StoredDispatch},
    pages::{
        num_traits::Zero,
        numerated::{iterators::IntervalIterator, tree::IntervalsTree},
        GearPage, WasmPage,
    },
    reservation::GasReserver,
    tasks::{ScheduledTask, TaskHandler},
};
use gear_core_errors::SignalCode;
use std::collections::BTreeMap;

impl JournalHandler for ExtManager {
    fn message_dispatched(
        &mut self,
        message_id: MessageId,
        _source: ActorId,
        outcome: DispatchOutcome,
    ) {
        match outcome {
            DispatchOutcome::MessageTrap { program_id, trap } => {
                log::debug!("🪤 Program {program_id} terminated with a trap: {trap}");
                self.failed.insert(message_id);
            }
            DispatchOutcome::NoExecution => {
                self.not_executed.insert(message_id);
            }
            DispatchOutcome::Success | DispatchOutcome::Exit { .. } => {
                self.succeed.insert(message_id);
            }
            DispatchOutcome::InitFailure {
                program_id, origin, ..
            } => {
                self.init_failure(program_id, origin);
                self.failed.insert(message_id);
            }
            DispatchOutcome::InitSuccess { program_id, .. } => {
                self.init_success(program_id);
                self.succeed.insert(message_id);
            }
        }
    }

    fn gas_burned(&mut self, message_id: MessageId, amount: u64) {
        log::debug!("Burned: {amount:?} from: {message_id:?}");

        self.gas_allowance = self.gas_allowance.saturating_sub(amount);
        self.spend_burned(message_id, amount);
    }

    fn exit_dispatch(&mut self, id_exited: ActorId, value_destination: ActorId) {
        log::debug!(
            "Exit dispatch: id_exited = {id_exited}, value_destination = {value_destination}"
        );

        self.waitlist.drain_key(id_exited).for_each(|entry| {
            let message = self.wake_dispatch_requirements(entry);

            self.dispatches.push_back(message);
        });

        Actors::modify(id_exited, |actor| {
            let actor =
                actor.unwrap_or_else(|| panic!("Can't find existing program {id_exited:?}"));

            if let TestActor::Initialized(program) =
                std::mem::replace(actor, TestActor::Exited(value_destination))
            {
                for (reservation_id, slot) in program.gas_reservation_map {
                    let slot = self.remove_gas_reservation_slot(reservation_id, slot);

                    let result = self.task_pool.delete(
                        slot.finish,
                        ScheduledTask::RemoveGasReservation(id_exited, reservation_id),
                    );
                    log::debug!(
                        "remove_gas_reservation_map; program_id = {id_exited:?}, result = {result:?}"
                    );
                }
            }
        });

        let value = Accounts::balance(id_exited);
        if value != 0 {
            Accounts::transfer(id_exited, value_destination, value, false);
        }
    }

    fn message_consumed(&mut self, message_id: MessageId) {
        self.consume_and_retrieve(message_id);
    }

    fn send_dispatch(
        &mut self,
        message_id: MessageId,
        dispatch: Dispatch,
        delay: u32,
        reservation: Option<ReservationId>,
    ) {
        let to_user = Actors::is_user(dispatch.destination())
            && !self.no_code_program.contains(&dispatch.destination());
        if delay > 0 {
            log::debug!(
                "[{message_id}] new delayed dispatch#{} with delay for {delay} blocks",
                dispatch.id()
            );

            self.send_delayed_dispatch(message_id, dispatch, delay, to_user, reservation);
            return;
        }

        log::debug!("[{message_id}] new dispatch#{}", dispatch.id());

        let source = dispatch.source();
        let is_program = Actors::is_program(dispatch.destination())
            || self.no_code_program.contains(&dispatch.destination());

        if is_program {
            let gas_limit = dispatch.gas_limit();
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
                self.bank.deposit_value(source, dispatch.value(), false);
            }

            match (gas_limit, reservation) {
                (Some(gas_limit), None) => self
                    .gas_tree
                    .split_with_value(dispatch.is_reply(), message_id, dispatch.id(), gas_limit)
                    .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e)),
                (None, None) => self
                    .gas_tree
                    .split(dispatch.is_reply(), message_id, dispatch.id())
                    .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e)),
                (None, Some(reservation)) => {
                    self.gas_tree
                        .split(dispatch.is_reply(), reservation, dispatch.id())
                        .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
                    self.remove_gas_reservation_with_task(dispatch.source(), reservation);
                }
                (Some(_), Some(_)) => unreachable!(
                    "Sending dispatch with gas limit from reservation \
                    is currently unimplemented and there is no way to send such dispatch"
                ),
            }

            self.dispatches.push_back(dispatch.into_stored());
        } else {
            log::debug!(
                "Sending user message {:?} from {:?} with gas limit {:?}",
                dispatch.message(),
                message_id,
                dispatch.gas_limit(),
            );
            self.send_user_message(message_id, dispatch.into_parts().1, reservation);
        }
    }

    fn wait_dispatch(
        &mut self,
        dispatch: StoredDispatch,
        duration: Option<u32>,
        waited_type: MessageWaitedType,
    ) {
        log::debug!("[{}] wait", dispatch.id());

        self.wait_dispatch_impl(
            dispatch,
            duration,
            MessageWaitedRuntimeReason::from(waited_type).into_reason(),
        );
    }

    fn wake_message(
        &mut self,
        message_id: MessageId,
        program_id: ActorId,
        awakening_id: MessageId,
        delay: u32,
    ) {
        log::debug!("[{message_id}] tries to wake message#{awakening_id}");

        if delay.is_zero() {
            if let Ok(dispatch) = self.wake_dispatch_impl(program_id, awakening_id) {
                self.dispatches.push_back(dispatch);

                return;
            }
        } else if self.waitlist.contains(program_id, awakening_id) {
            let expected_bn = self.block_height() + delay;
            let task = ScheduledTask::WakeMessage(program_id, awakening_id);

            // This validation helps us to avoid returning error on insertion into
            // `TaskPool` in case of duplicate wake.
            if !self.task_pool.contains(&expected_bn, &task) {
                self.task_pool.add(expected_bn, task).unwrap_or_else(|e| {
                    let err_msg = format!(
                        "JournalHandler::wake_message: failed adding task for waking message. \
                        Expected bn - {expected_bn:?}, program id - {program_id}, message_id - {awakening_id}.
                        Got error - {e:?}"
                    );

                    unreachable!("{err_msg}");
                });
                self.on_task_pool_change();
            }

            return;
        }

        log::debug!("Failed to wake unknown message {awakening_id:?} from {message_id:?}");
    }

    fn update_pages_data(&mut self, program_id: ActorId, pages_data: BTreeMap<GearPage, PageBuf>) {
        self.update_storage_pages(&program_id, pages_data);
    }

    fn update_allocations(&mut self, program_id: ActorId, allocations: IntervalsTree<WasmPage>) {
        self.update_program(program_id, |program| {
            program
                .allocations
                .difference(&allocations)
                .flat_map(IntervalIterator::from)
                .flat_map(|page| page.to_iter())
                .for_each(|ref page| {
                    program.pages_data.remove(page);
                });
            program.allocations = allocations;
        })
        .expect("no genuine program was found");
    }

    fn send_value(&mut self, from: ActorId, to: ActorId, value: Value, locked: bool) {
        if value.is_zero() {
            // Nothing to do
            return;
        }

        if locked {
            self.bank.transfer_locked_value(from, to, value);
        } else {
            self.bank.transfer_value(from, to, value);
        }
    }

    fn store_new_programs(
        &mut self,
        program_id: ActorId,
        code_id: CodeId,
        candidates: Vec<(MessageId, ActorId)>,
    ) {
        if let Some(code) = self.opt_binaries.get(&code_id).cloned() {
            for (init_message_id, candidate_id) in candidates {
                if !Actors::contains_key(candidate_id) {
                    let (instrumented, _) =
                        ProgramBuilder::build_instrumented_code_and_id(code.clone());
                    self.store_new_actor(
                        candidate_id,
                        Program {
                            code: instrumented,
                            code_id,
                            allocations: Default::default(),
                            pages_data: Default::default(),
                            gas_reservation_map: Default::default(),
                        },
                        Some(init_message_id),
                    );

                    // Transfer the ED from the program-creator to the new program
                    Accounts::transfer(program_id, candidate_id, EXISTENTIAL_DEPOSIT, true);
                } else {
                    log::debug!("Program with id {candidate_id:?} already exists");
                }
            }
        } else {
            log::debug!("No referencing code with code hash {code_id:?} for candidate programs");
            for (_, invalid_candidate_id) in candidates {
                self.no_code_program.insert(invalid_candidate_id);
            }
        }
    }

    fn stop_processing(&mut self, dispatch: StoredDispatch, gas_burned: u64) {
        log::debug!(
            "Not enough gas for processing msg id {}, allowance equals {}, gas tried to burn at least {}",
            dispatch.id(),
            self.gas_allowance,
            gas_burned,
        );
        self.gas_allowance = self.gas_allowance.saturating_sub(gas_burned);
        self.messages_processing_enabled = false;
        self.dispatches.push_front(dispatch);
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

        let hold = HoldBoundBuilder::new(StorageType::Reservation).duration(self, duration);

        if hold.expected_duration(self).is_zero() {
            let err_msg = format!(
                "JournalHandler::reserve_gas: reservation got zero duration hold bound for storing. \
                Duration - {duration}, block cost - {cost}, program - {program_id}.",
                cost = Self::cost_by_storage_type(StorageType::Reservation)
            );

            unreachable!("{err_msg}");
        }

        let total_amount = amount.saturating_add(hold.lock_amount(self));

        self.gas_tree
            .reserve_gas(message_id, reservation_id, total_amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted: {:?}", e));

        let lock_id = hold.lock_id().unwrap_or_else(|| {
            // Reservation storage is guaranteed to have an associated lock id
            let err_msg =
                "JournalHandler::reserve_gas: No associated lock id for the reservation storage";

            unreachable!("{err_msg}");
        });

        self.gas_tree
            .lock(reservation_id, lock_id, hold.lock_amount(self))
            .unwrap_or_else(|e| {
                let err_msg = format!(
                    "JournalHandler::reserve_gas: failed locking gas for the reservation hold. \
                Reseravation - {reservation_id}, lock amount - {lock}. Got error - {e:?}",
                    lock = hold.lock_amount(self)
                );

                unreachable!("{err_msg}");
            });

        self.task_pool.add(
            hold.expected(),
            ScheduledTask::RemoveGasReservation(program_id, reservation_id)
        ).unwrap_or_else(|e| {
            let err_msg = format!(
                "JournalHandler::reserve_gas: failed adding task for gas reservation removal. \
                Expected bn - {bn:?}, program id - {program_id}, reservation id - {reservation_id}. Got error - {e:?}",
                bn = hold.expected()
            );
            unreachable!("{err_msg}");
        });
        self.on_task_pool_change();
    }

    fn unreserve_gas(
        &mut self,
        reservation_id: ReservationId,
        program_id: ActorId,
        expiration: u32,
    ) {
        <Self as TaskHandler<ActorId, MessageId, bool>>::remove_gas_reservation(
            self,
            program_id,
            reservation_id,
        );

        let _ = self
            .task_pool
            .delete(
                expiration,
                ScheduledTask::RemoveGasReservation(program_id, reservation_id),
            )
            .map(|_| {
                self.on_task_pool_change();
            });
    }

    fn update_gas_reservation(&mut self, program_id: ActorId, reserver: GasReserver) {
        let block_height = self.block_height();
        self.update_program(program_id, |program| {
            program.gas_reservation_map =
                reserver.into_map(block_height, |duration| block_height + duration);
        })
        .expect("no genuine program was found");
    }

    fn system_reserve_gas(&mut self, message_id: MessageId, amount: u64) {
        log::debug!("Reserve {amount} of gas for system from {message_id}");

        self.gas_tree
            .system_reserve(message_id, amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted: {:?}", e));
    }

    fn system_unreserve_gas(&mut self, message_id: MessageId) {
        let amount = self
            .gas_tree
            .system_unreserve(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted: {:?}", e));

        if amount != 0 {
            log::debug!("Unreserved {amount} gas for system from {message_id}");
        } else {
            log::debug!(
                "Gas for system was not unreserved from {message_id} as there is no supply",
            );
        }
    }

    fn send_signal(&mut self, message_id: MessageId, destination: ActorId, code: SignalCode) {
        let reserved = self
            .gas_tree
            .system_unreserve(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        if reserved != 0 {
            log::debug!(
                "Send signal issued by {message_id} to {destination} with {reserved} supply",
            );

            let trap_signal = SignalMessage::new(message_id, code)
                .into_dispatch(message_id, destination)
                .into_stored();

            self.gas_tree
                .split_with_value(
                    trap_signal.is_reply(),
                    message_id,
                    trap_signal.id(),
                    reserved,
                )
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            self.dispatches.push_back(trap_signal);
        } else {
            log::trace!("Signal wasn't send due to inappropriate supply");
        }
    }

    fn reply_deposit(&mut self, message_id: MessageId, future_reply_id: MessageId, amount: u64) {
        log::debug!("Creating reply deposit {amount} gas for message id {future_reply_id}");

        self.gas_tree
            .create_deposit(message_id, future_reply_id, amount)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
    }
}
