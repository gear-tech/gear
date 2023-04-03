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

pub use journal::*;
pub use task::*;

use crate::{
    Config, CostsPerBlockOf, CurrencyOf, GasHandlerOf, Pallet, ProgramStorageOf, QueueOf,
    TaskPoolOf,
};
use common::{
    event::*,
    scheduler::{ScheduledTask, SchedulingCostsPerBlock, TaskHandler, TaskPool},
    storage::{Interval, Queue},
    ActiveProgram, CodeStorage, GasTree, Origin, ProgramState, ProgramStorage,
};
use core::fmt;
use core_processor::common::{Actor, ExecutableActorData};
use frame_support::{
    codec::{Decode, Encode},
    traits::Currency,
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::{
    code::{CodeAndId, InstrumentedCode},
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::WasmPage,
    message::{DispatchKind, SignalMessage, StatusCode},
    reservation::GasReservationSlot,
};
use gear_core_errors::SimpleSignalError;
use primitive_types::H256;
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    convert::{TryFrom, TryInto},
    marker::PhantomData,
    prelude::*,
};

#[derive(Clone, Decode, Encode)]
pub enum HandleKind {
    Init(Vec<u8>),
    InitByHash(CodeId),
    Handle(ProgramId),
    Reply(MessageId, StatusCode),
    Signal(MessageId, StatusCode),
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
        } else if ProgramStorageOf::<T>::program_exists(*id) {
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
        let active: ActiveProgram = ProgramStorageOf::<T>::get_program(id)?.0.try_into().ok()?;
        let code_id = CodeId::from_origin(active.code_hash);

        let balance =
            CurrencyOf::<T>::free_balance(&<T::AccountId as Origin>::from_origin(id.into_origin()))
                .unique_saturated_into();

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
            }),
        })
    }

    pub fn set_program(
        &self,
        program_id: ProgramId,
        code_info: &CodeInfo,
        message_id: MessageId,
        block_number: <T as frame_system::Config>::BlockNumber,
    ) {
        // Program can be added to the storage only with code, which is done in
        // `submit_program` or `upload_code` extrinsic.
        //
        // Code can exist without program, but the latter can't exist without code.
        debug_assert!(
            T::CodeStorage::exists(CodeId::from_origin(code_info.id)),
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
        };

        ProgramStorageOf::<T>::add_program(program_id, program, block_number)
            .expect("set_program shouldn't be called for the existing id");
    }

    fn clean_reservation_tasks(&mut self, program_id: ProgramId, maybe_inactive: bool) {
        let maybe_active_program = ProgramStorageOf::<T>::get_program(program_id)
            .and_then(|(p, _bn)| ActiveProgram::try_from(p).ok());

        if maybe_active_program.is_none() && maybe_inactive {
            return;
        };

        let active_program = maybe_active_program.unwrap_or_else(|| {
            unreachable!("Clean reservations can only be called on active program")
        });

        for (reservation_id, reservation_slot) in active_program.gas_reservation_map {
            <Self as TaskHandler<T::AccountId>>::remove_gas_reservation(
                self,
                program_id,
                reservation_id,
            );

            let _ = TaskPoolOf::<T>::delete(
                BlockNumberFor::<T>::from(reservation_slot.finish),
                ScheduledTask::RemoveGasReservation(program_id, reservation_id),
            );
        }
    }

    pub fn remove_gas_reservation_impl(
        program_id: ProgramId,
        reservation_id: ReservationId,
    ) -> GasReservationSlot {
        let slot = ProgramStorageOf::<T>::update_active_program(program_id, |p, _bn| {
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

        GasHandlerOf::<T>::unlock_all(reservation_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        let interval = Interval {
            start: BlockNumberFor::<T>::from(slot.start),
            finish: BlockNumberFor::<T>::from(slot.finish),
        };

        Pallet::<T>::charge_for_hold(
            reservation_id,
            interval,
            CostsPerBlockOf::<T>::reservation(),
        );

        Pallet::<T>::consume_and_retrieve(reservation_id);

        slot
    }

    fn send_signal(
        &mut self,
        message_id: MessageId,
        destination: ProgramId,
        err: SimpleSignalError,
    ) {
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
            let trap_signal = SignalMessage::new(message_id, err)
                .into_dispatch(message_id, destination)
                .into_stored();

            // Splitting gas for newly created reply message.
            // TODO: don't split (#1743)
            GasHandlerOf::<T>::split_with_value(message_id, trap_signal.id(), reserved)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            // Enqueueing dispatch into message queue.
            QueueOf::<T>::queue(trap_signal)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
        } else {
            log::trace!("Signal wasn't sent due to inappropriate supply");
        }
    }
}
