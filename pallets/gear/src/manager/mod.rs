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

//! Manager which handles results of message processing.
//!
//! Should be mentioned, that if message contains value we have a guarantee that it will be sent further in case of successful execution,
//! or sent back in case execution ends up with an error. This guarantee is reached by the following conditions:
//! 1. **Reserve/unreserve model for transferring values**.
//!    Ownership over message value is moved not by simple transfer operation, which decreases **free** balance of sender. That is done by
//!    reserving value before message is executed and repatriating reserved in favor of beneficiary in case of successful execution, or unreserving
//!    in case of execution resulting in a trap. So, it gives us a guarantee that regardless of the result of message execution, there is **always some
//!    value** to perform asset management, i.e move tokens further to the recipient or give back to sender. The guarantee is implemented by using
//!    corresponding `pallet_balances` functions (`reserve`, `repatriate_reserved`, `unreserve` along with `transfer`) in `pallet_gear` extrinsics,
//!    [`JournalHandler::send_dispatch`](core_processor::common::JournalHandler::send_dispatch) and
//!    [`JournalHandler::send_value`](core_processor::common::JournalHandler::send_value) procedures.
//!
//! 2. **Balance sufficiency before adding message with value to the queue**.
//!    Before message is added to the queue, sender's balance is checked for having adequate amount of assets to send desired value. For actors, who
//!    can sign transactions, these checks are done in extrinsic calls. For programs these checks are done on core backend level during execution. In details,
//!    when a message is executed, it has some context, which is set from the pallet level, and a part of the context data is program's actual balance (current balance +
//!    value sent within the executing message). So if during execution of the original message some other messages were sent, message send call is followed
//!    by program's balance checks. The check gives guarantee that value reservation call in
//!
//! [`JournalHandler::send_dispatch`](core_processor::common::JournalHandler::send_dispatch) for program's messages won't fail, because there is always a
//! sufficient balance for the call.
//!
//! 3. **Messages's value management considers existential deposit rule**.
//!    It means that before message with value is added to the queue, value is checked to be in the valid range - `{0} ∪ [existential_deposit; +inf)`. This is
//!    crucial for programs. The check gives guarantee that if funds were moved to the program, the program will definitely have an account in `pallet_balances`
//!    registry and will be able then to manage these funds. Without this check, program could receive funds, but won't be able to use them.
//!
//! Due to these 3 conditions implemented in `pallet_gear`, we have a guarantee that value management calls, performed by user or program, won't fail.

mod journal;
mod task;

use gear_core_errors::{ReplyCode, SignalCode};
pub use task::*;

use crate::{
    fungible, BuiltinDispatcherFactory, Config, CurrencyOf, Event, Fortitude, GasHandlerOf, Pallet,
    Preservation, ProgramStorageOf, QueueOf, TaskPoolOf, WaitlistOf, EXISTENTIAL_DEPOSIT_LOCK_ID,
};
use alloc::format;
use common::{
    event::*,
    scheduler::{StorageType, TaskPool},
    storage::{Interval, IterableByKeyMap, Queue},
    CodeStorage, Origin, ProgramStorage, ReservableTree,
};
use core::{fmt, mem};
use frame_support::traits::{Currency, ExistenceRequirement, LockableCurrency};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::{
    ids::{ActorId, CodeId, MessageId, ReservationId},
    message::SignalMessage,
    program::{ActiveProgram, Program, ProgramState},
    reservation::GasReservationSlot,
    tasks::ScheduledTask,
};
use scale_info::TypeInfo;
use sp_runtime::{
    codec::{Decode, Encode},
    traits::Zero,
};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    marker::PhantomData,
    prelude::*,
};

#[derive(Clone, Decode, Encode, TypeInfo)]
pub enum HandleKind {
    Init(Vec<u8>),
    InitByHash(CodeId),
    Handle(ActorId),
    Reply(MessageId, ReplyCode),
    Signal(MessageId, SignalCode),
}

impl fmt::Debug for HandleKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HandleKind::Init(_) => f.debug_tuple("Init").field(&format_args!("[...]")).finish(),
            HandleKind::InitByHash(id) => f.debug_tuple("InitByHash").field(id).finish(),
            HandleKind::Handle(id) => f.debug_tuple("Handle").field(id).finish(),
            HandleKind::Reply(id, code) => f.debug_tuple("Reply").field(id).field(code).finish(),
            HandleKind::Signal(id, code) => f.debug_tuple("Signal").field(id).field(code).finish(),
        }
    }
}

/// Journal handler implementation for `pallet_gear`.
pub struct ExtManager<T: Config> {
    /// Ids checked that they are users.
    users: BTreeSet<ActorId>,
    /// Ids checked that they are programs.
    programs: BTreeSet<ActorId>,
    /// Messages dispatches.
    dispatch_statuses: BTreeMap<MessageId, DispatchStatus>,
    /// Programs, which state changed.
    state_changes: BTreeSet<ActorId>,
    /// Builtin programs.
    builtins: <T::BuiltinDispatcherFactory as BuiltinDispatcherFactory>::Output,
    /// Phantom data for generic usage.
    _phantom: PhantomData<T>,
}

/// Data need for depositing event about queue processing result.
pub struct QueuePostProcessingData {
    /// Message dispatches results.
    pub dispatch_statuses: BTreeMap<MessageId, DispatchStatus>,
    /// Programs, which state changed.
    pub state_changes: BTreeSet<ActorId>,
}

impl<T: Config> From<ExtManager<T>> for QueuePostProcessingData {
    fn from(ext_manager: ExtManager<T>) -> Self {
        Self {
            dispatch_statuses: ext_manager.dispatch_statuses,
            state_changes: ext_manager.state_changes,
        }
    }
}

impl<T: Config> ExtManager<T>
where
    T::AccountId: Origin,
{
    pub fn new(
        builtins: <T::BuiltinDispatcherFactory as BuiltinDispatcherFactory>::Output,
    ) -> Self {
        Self {
            _phantom: PhantomData,
            users: Default::default(),
            programs: Default::default(),
            dispatch_statuses: Default::default(),
            state_changes: Default::default(),
            builtins,
        }
    }

    pub fn builtins(&self) -> &<T::BuiltinDispatcherFactory as BuiltinDispatcherFactory>::Output {
        &self.builtins
    }

    /// Check if id is program and save result.
    pub fn check_program_id(&mut self, id: &ActorId) -> bool {
        // TODO: research how much need to charge for `program_exists` query.
        if self.programs.contains(id) {
            true
        } else if self.users.contains(id) {
            false
        } else if Pallet::<T>::program_exists(&self.builtins, *id) {
            self.programs.insert(*id);
            true
        } else {
            self.users.insert(*id);
            false
        }
    }

    /// Check if id is user and save result.
    pub fn check_user_id(&mut self, id: &ActorId) -> bool {
        !self.check_program_id(id)
    }

    pub fn set_program(
        &self,
        program_id: ActorId,
        code_id: CodeId,
        message_id: MessageId,
        expiration_block: BlockNumberFor<T>,
    ) {
        // Program can be added to the storage only with code, which is done in
        // `submit_program` or `upload_code` extrinsic.
        //
        // Code can exist without program, but the latter can't exist without code.
        debug_assert!(
            T::CodeStorage::original_code_exists(code_id),
            "Program set must be called only when code exists",
        );

        // An empty program has been just constructed: it contains no mem allocations.
        let program = ActiveProgram {
            allocations_tree_len: 0,
            code_id,
            state: ProgramState::Uninitialized { message_id },
            gas_reservation_map: Default::default(),
            expiration_block,
            memory_infix: Default::default(),
        };

        ProgramStorageOf::<T>::add_program(program_id, program)
            .expect("set_program shouldn't be called for the existing id");
    }

    fn remove_gas_reservation_slot(
        reservation_id: ReservationId,
        slot: GasReservationSlot,
    ) -> GasReservationSlot {
        let interval = Interval {
            start: BlockNumberFor::<T>::from(slot.start),
            finish: BlockNumberFor::<T>::from(slot.finish),
        };

        Pallet::<T>::charge_for_hold(reservation_id, interval, StorageType::Reservation);

        Pallet::<T>::consume_and_retrieve(reservation_id);

        slot
    }

    pub fn remove_gas_reservation_impl(
        program_id: ActorId,
        reservation_id: ReservationId,
    ) -> GasReservationSlot {
        let slot = ProgramStorageOf::<T>::update_active_program(program_id, |p| {
            p.gas_reservation_map
                .remove(&reservation_id)
                .unwrap_or_else(|| {
                    let err_msg = format!(
                        "ExtManager::remove_gas_reservation_impl: failed removing gas reservation. \
                    Reservation {reservation_id} doesn't exist."
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                })
        })
        .unwrap_or_else(|e| {
            // Guaranteed to be called on existing program
            let err_msg = format!(
                "ExtManager::remove_gas_reservation_impl: failed to update program. \
            Program - {program_id}. Got error: {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });

        Self::remove_gas_reservation_slot(reservation_id, slot)
    }

    fn remove_gas_reservation_map(
        program_id: ActorId,
        gas_reservation_map: BTreeMap<ReservationId, GasReservationSlot>,
    ) {
        for (reservation_id, slot) in gas_reservation_map {
            let slot = Self::remove_gas_reservation_slot(reservation_id, slot);

            let result = TaskPoolOf::<T>::delete(
                BlockNumberFor::<T>::from(slot.finish),
                ScheduledTask::RemoveGasReservation(program_id, reservation_id),
            );

            log::debug!(
                "remove_gas_reservation_map; program_id = {program_id:?}, result = {result:?}"
            );
        }
    }

    fn send_signal(&mut self, message_id: MessageId, destination: ActorId, code: SignalCode) {
        let reserved = GasHandlerOf::<T>::system_unreserve(message_id).unwrap_or_else(|e| {
            let err_msg = format!(
                "ExtManager::send_signal: failed system unreserve. \
                Message id - {message_id}. Got error: {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });
        if reserved != 0 {
            log::debug!(
                "Send signal issued by {message_id} to {destination} with {reserved} supply"
            );

            // Creating signal message.
            let trap_signal = SignalMessage::new(message_id, code)
                .into_dispatch(message_id, destination)
                .into_stored();

            // Splitting gas for newly created signal message.
            Pallet::<T>::split_with_value(
                message_id,
                trap_signal.id(),
                reserved,
                trap_signal.is_reply(),
            );

            // Enqueueing dispatch into message queue.
            QueueOf::<T>::queue(trap_signal).unwrap_or_else(|e| {
                let err_msg =
                    format!("ExtManager::send_signal: failed queuing message. Got error - {e:?}");

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });
        } else {
            log::trace!("Signal wasn't sent due to inappropriate supply");
        }
    }

    /// Removes reservation map and memory pages of the program
    fn clean_inactive_program(
        program_id: ActorId,
        program: &mut ActiveProgram<BlockNumberFor<T>>,
        value_destination: ActorId,
    ) {
        Self::remove_gas_reservation_map(program_id, mem::take(&mut program.gas_reservation_map));

        let program_account = program_id.cast();
        let value_destination = value_destination.cast();

        // Remove the ED lock to allow the account to be reaped.
        CurrencyOf::<T>::remove_lock(EXISTENTIAL_DEPOSIT_LOCK_ID, &program_account);

        // The `reducible_balance` should now include the ED since no consumer is left.
        // If some part of the program account's `free` balance is still `frozen` for some reason
        // it will be offset against the `reducible_balance`.
        let balance = <CurrencyOf<T> as fungible::Inspect<_>>::reducible_balance(
            &program_account,
            Preservation::Expendable,
            Fortitude::Polite,
        );
        if !balance.is_zero() {
            // The transfer is guaranteed to succeed since the amount contains at least the ED
            // from the deactivated program.
            CurrencyOf::<T>::transfer(
                &program_account,
                &value_destination,
                balance,
                ExistenceRequirement::AllowDeath,
            )
            .unwrap_or_else(|e| {
                let err_msg = format!("ExtManager::clean_inactive_program: failed transferring the rest balance. \
                Sender - {program_account:?}, sender balance - {balance:?}, dest - {value_destination:?}. \
                Got error: {e:?}");

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });
        }
    }

    /// Removes all messages to `program_id` from the waitlist.
    fn clean_waitlist(program_id: ActorId) {
        let reason = MessageWokenSystemReason::ProgramGotInitialized.into_reason();

        WaitlistOf::<T>::drain_key(program_id).for_each(|entry| {
            let message = Pallet::<T>::wake_dispatch_requirements(entry, reason.clone());

            QueueOf::<T>::queue(message)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {e:?}"));
        });
    }

    fn process_failed_init(program_id: ActorId, origin: ActorId) {
        // Waitlist can have messages only in one case of failed init:
        // that's when program initialization message went to waitlist (say, because of async call),
        // then the program receives reply (which queue allows to process for uninitialized program),
        // which itself ends up being in waitlist (a wait syscall is invoked in `handle_reply`).
        Self::clean_waitlist(program_id);

        let _ = ProgramStorageOf::<T>::update_program_if_active(program_id, |p, bn| {
            let _ = TaskPoolOf::<T>::delete(bn, ScheduledTask::PauseProgram(program_id));

            if let Program::Active(program) = p {
                Self::clean_inactive_program(program_id, program, origin);
            }

            *p = Program::Terminated(origin);
        });

        Pallet::<T>::deposit_event(Event::ProgramChanged {
            id: program_id,
            change: ProgramChangeKind::Terminated,
        });
    }
}
