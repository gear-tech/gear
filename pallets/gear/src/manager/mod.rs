// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
//! Ownership over message value is moved not by simple transfer operation, which decreases **free** balance of sender. That is done by
//! reserving value before message is executed and repatriating reserved in favor of beneficiary in case of successful execution, or unreserving
//! in case of execution resulting in a trap. So, it gives us a guarantee that regardless of the result of message execution, there is **always some
//! value** to perform asset management, i.e move tokens further to the recipient or give back to sender. The guarantee is implemented by using
//! corresponding `pallet_balances` functions (`reserve`, `repatriate_reserved`, `unreserve` along with `transfer`) in `pallet_gear` extrinsics,
//! [`JournalHandler::send_dispatch`](core_processor::common::JournalHandler::send_dispatch) and
//! [`JournalHandler::send_value`](core_processor::common::JournalHandler::send_value) procedures.
//!
//! 2. **Balance sufficiency before adding message with value to the queue**.
//! Before message is added to the queue, sender's balance is checked for having adequate amount of assets to send desired value. For actors, who
//! can sign transactions, these checks are done in extrinsic calls. For programs these checks are done on core backend level during execution. In details,
//! when a message is executed, it has some context, which is set from the pallet level, and a part of the context data is program's actual balance (current balance +
//! value sent within the executing message). So if during execution of the original message some other messages were sent, message send call is followed
//! by program's balance checks. The check gives guarantee that value reservation call in
//! [`JournalHandler::send_dispatch`](core_processor::common::JournalHandler::send_dispatch) for program's messages won't fail, because there is always a
//! sufficient balance for the call.
//!
//! 3. **Messages's value management considers existential deposit rule**.
//! It means that before message with value is added to the queue, value is checked to be in the valid range - `{0} ∪ [existential_deposit; +inf)`. This is
//! crucial for programs. The check gives guarantee that if funds were moved to the program, the program will definitely have an account in `pallet_balances`
//! registry and will be able then to manage these funds. Without this check, program could receive funds, but won't be able to use them.
//!
//! Due to these 3 conditions implemented in `pallet_gear`, we have a guarantee that value management calls, performed by user or program, won't fail.

mod journal;
mod task;

use gear_core_errors::{ReplyCode, SignalCode};
pub use journal::*;
pub use task::*;

use crate::{
    Config, CurrencyOf, Event, GasHandlerOf, Pallet, ProgramStorageOf, QueueOf, TaskPoolOf,
    WaitlistOf,
};
use common::{
    event::*,
    scheduler::{ScheduledTask, StorageType, TaskPool},
    storage::{Interval, IterableByKeyMap, Queue},
    ActiveProgram, CodeStorage, Origin, Program, ProgramState, ProgramStorage, ReservableTree,
};
use core::fmt;
use core_processor::common::{Actor, ExecutableActorData};
use frame_support::{
    codec::{Decode, Encode},
    traits::{Currency, ExistenceRequirement},
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::{
    code::{CodeAndId, InstrumentedCode},
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    message::{DispatchKind, SignalMessage},
    pages::WasmPage,
    program::MemoryInfix,
    reservation::GasReservationSlot,
};
use primitive_types::H256;
use sp_runtime::traits::{UniqueSaturatedInto, Zero};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    convert::TryInto,
    marker::PhantomData,
    prelude::*,
};

#[derive(Clone, Decode, Encode)]
pub enum HandleKind {
    Init(Vec<u8>),
    InitByHash(CodeId),
    Handle(ProgramId),
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

#[derive(Debug)]
pub struct CodeInfo {
    id: H256,
    exports: BTreeSet<DispatchKind>,
    static_pages: WasmPage,
}

impl CodeInfo {
    pub fn from_code_and_id(code: &CodeAndId) -> Self {
        Self {
            id: code.code_id().into_origin(),
            exports: code.code().exports().clone(),
            static_pages: code.code().static_pages(),
        }
    }

    pub fn from_code(id: &CodeId, code: &InstrumentedCode) -> Self {
        Self {
            id: id.into_origin(),
            exports: code.exports().clone(),
            static_pages: code.static_pages(),
        }
    }
}

/// Journal handler implementation for `pallet_gear`.
#[derive(Clone)]
pub struct ExtManager<T: Config> {
    /// Ids checked that they are users.
    users: BTreeSet<ProgramId>,
    /// Ids checked that they are programs.
    programs: BTreeSet<ProgramId>,
    /// Ids of programs which memory pages have been loaded earlier during processing a block.
    program_loaded_pages: BTreeSet<ProgramId>,
    /// Messages dispatches.
    dispatch_statuses: BTreeMap<MessageId, DispatchStatus>,
    /// Programs, which state changed.
    state_changes: BTreeSet<ProgramId>,
    /// Phantom data for generic usage.
    _phantom: PhantomData<T>,
}

/// Data need for depositing event about queue processing result.
pub struct QueuePostProcessingData {
    /// Message dispatches results.
    pub dispatch_statuses: BTreeMap<MessageId, DispatchStatus>,
    /// Programs, which state changed.
    pub state_changes: BTreeSet<ProgramId>,
}

impl<T: Config> From<ExtManager<T>> for QueuePostProcessingData {
    fn from(ext_manager: ExtManager<T>) -> Self {
        Self {
            dispatch_statuses: ext_manager.dispatch_statuses,
            state_changes: ext_manager.state_changes,
        }
    }
}

impl<T: Config> Default for ExtManager<T>
where
    T::AccountId: Origin,
{
    fn default() -> Self {
        ExtManager {
            _phantom: PhantomData,
            users: Default::default(),
            programs: Default::default(),
            program_loaded_pages: Default::default(),
            dispatch_statuses: Default::default(),
            state_changes: Default::default(),
        }
    }
}

impl<T: Config> ExtManager<T>
where
    T::AccountId: Origin,
{
    /// Check if id is program and save result.
    pub fn check_program_id(&mut self, id: &ProgramId) -> bool {
        // TODO: research how much need to charge for `program_exists` query.
        if self.programs.contains(id) {
            true
        } else if self.users.contains(id) {
            false
        } else if Pallet::<T>::program_exists(*id) {
            self.programs.insert(*id);
            true
        } else {
            self.users.insert(*id);
            false
        }
    }

    /// Check if id is user and save result.
    pub fn check_user_id(&mut self, id: &ProgramId) -> bool {
        !self.check_program_id(id)
    }

    /// Checks if memory pages of a program were loaded.
    pub fn program_pages_loaded(&self, id: &ProgramId) -> bool {
        self.program_loaded_pages.contains(id)
    }

    /// Adds program's id to the collection of programs with
    /// loaded memory pages.
    pub fn insert_program_id_loaded_pages(&mut self, id: ProgramId) {
        debug_assert!(self.check_program_id(&id));

        self.program_loaded_pages.insert(id);
    }
    /// NOTE: By calling this function we can't differ whether `None` returned, because
    /// program with `id` doesn't exist or it's terminated
    pub fn get_actor(&self, id: ProgramId) -> Option<Actor> {
        let active: ActiveProgram<_> = ProgramStorageOf::<T>::get_program(id)?.try_into().ok()?;
        let code_id = active.code_hash.cast();

        let balance = CurrencyOf::<T>::free_balance(&id.cast()).unique_saturated_into();

        Some(Actor {
            balance,
            destination_program: id,
            executable_data: Some(ExecutableActorData {
                allocations: active.allocations.clone(),
                code_id,
                code_exports: active.code_exports,
                static_pages: active.static_pages,
                initialized: matches!(active.state, ProgramState::Initialized),
                pages_with_data: active.pages_with_data,
                gas_reservation_map: active.gas_reservation_map,
                memory_infix: active.memory_infix,
            }),
        })
    }

    pub fn set_program(
        &self,
        program_id: ProgramId,
        code_info: &CodeInfo,
        message_id: MessageId,
        expiration_block: BlockNumberFor<T>,
    ) {
        // Program can be added to the storage only with code, which is done in
        // `submit_program` or `upload_code` extrinsic.
        //
        // Code can exist without program, but the latter can't exist without code.
        debug_assert!(
            T::CodeStorage::exists(code_info.id.cast()),
            "Program set must be called only when code exists",
        );

        // An empty program has been just constructed: it contains no mem allocations.
        let program = common::ActiveProgram {
            allocations: Default::default(),
            pages_with_data: Default::default(),
            code_hash: code_info.id,
            code_exports: code_info.exports.clone(),
            static_pages: code_info.static_pages,
            state: common::ProgramState::Uninitialized { message_id },
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
        program_id: ProgramId,
        reservation_id: ReservationId,
    ) -> GasReservationSlot {
        let slot = ProgramStorageOf::<T>::update_active_program(program_id, |p| {
            p.gas_reservation_map
                .remove(&reservation_id)
                .unwrap_or_else(|| {
                    unreachable!(
                        "Gas reservation removing called on non-existing reservation ID: {}",
                        reservation_id
                    )
                })
        })
        .unwrap_or_else(|e| {
            unreachable!(
                "Gas reservation removing guaranteed to be called only on existing program: {:?}",
                e
            )
        });

        Self::remove_gas_reservation_slot(reservation_id, slot)
    }

    pub fn remove_gas_reservation_map(
        program_id: ProgramId,
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

    fn send_signal(&mut self, message_id: MessageId, destination: ProgramId, code: SignalCode) {
        let reserved = GasHandlerOf::<T>::system_unreserve(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
        if reserved != 0 {
            log::debug!(
                "Send signal issued by {} to {} with {} supply",
                message_id,
                destination,
                reserved
            );

            // Creating signal message.
            let trap_signal = SignalMessage::new(message_id, code)
                .into_dispatch(message_id, destination)
                .into_stored();

            // Splitting gas for newly created reply message.
            // TODO: don't split (#1743)
            Pallet::<T>::split_with_value(
                message_id,
                trap_signal.id(),
                reserved,
                trap_signal.is_reply(),
            );

            // Enqueueing dispatch into message queue.
            QueueOf::<T>::queue(trap_signal)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
        } else {
            log::trace!("Signal wasn't sent due to inappropriate supply");
        }
    }

    /// Removes memory pages of the program and transfers program balance to the `value_destination`.
    fn clean_inactive_program(
        program_id: ProgramId,
        memory_infix: MemoryInfix,
        value_destination: ProgramId,
    ) {
        ProgramStorageOf::<T>::remove_program_pages(program_id, memory_infix);

        let program_account = program_id.cast();
        let balance = CurrencyOf::<T>::free_balance(&program_account);
        if !balance.is_zero() {
            let destination = Pallet::<T>::inheritor_for(value_destination).cast();

            CurrencyOf::<T>::transfer(
                &program_account,
                &destination,
                balance,
                ExistenceRequirement::AllowDeath,
            )
            .unwrap_or_else(|e| unreachable!("Failed to transfer value: {e:?}"));
        }
    }

    /// Removes all messages to `program_id` from the waitlist.
    fn clean_waitlist(program_id: ProgramId) {
        let reason = MessageWokenSystemReason::ProgramGotInitialized.into_reason();

        WaitlistOf::<T>::drain_key(program_id).for_each(|entry| {
            let message = Pallet::<T>::wake_dispatch_requirements(entry, reason.clone());

            QueueOf::<T>::queue(message)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {e:?}"));
        });

        ProgramStorageOf::<T>::waiting_init_remove(program_id);
    }

    fn process_failed_init(program_id: ProgramId, origin: ProgramId, executed: bool) {
        // Some messages addressed to the program could be processed
        // in the queue before init message. For example, that could
        // happen when init message had more gas limit then rest block
        // gas allowance, but a dispatch message to the program was
        // dequeued. The other case is async init.
        Self::clean_waitlist(program_id);

        ProgramStorageOf::<T>::update_program_if_active(program_id, |p, bn| {
            let _ = TaskPoolOf::<T>::delete(bn, ScheduledTask::PauseProgram(program_id));

            match p {
                Program::Active(program) => {
                    Self::remove_gas_reservation_map(
                        program_id,
                        core::mem::take(&mut program.gas_reservation_map),
                    );

                    Self::clean_inactive_program(program_id, program.memory_infix, origin);
                }
                _ if executed => unreachable!("Action executed only for active program"),
                _ => (),
            }

            *p = Program::Terminated(origin);
        })
        .unwrap_or_else(|e| {
            // If we run into `InitFailure` after real execution (not
            // prepare or precharge) processor methods, then we are
            // sure that it was active program.
            if executed {
                unreachable!(
                    "Program terminated status may only be set to an existing active program: {:?}",
                    e,
                );
            }
        });

        Pallet::<T>::deposit_event(Event::ProgramChanged {
            id: program_id,
            change: ProgramChangeKind::Terminated,
        });
    }
}
