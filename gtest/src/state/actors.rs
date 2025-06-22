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

//! Actors storage.

use std::{cell::RefCell, collections::BTreeMap, fmt};

use core_processor::common::ExecutableActorData;
use gear_common::{ActorId, CodeId, GearPage, MessageId, PageBuf};
use gear_core::{
    code::InstrumentedCode,
    pages::{numerated::tree::IntervalsTree, WasmPage},
    reservation::GasReservationMap,
};

thread_local! {
    static ACTORS_STORAGE: RefCell<BTreeMap<ActorId, TestActor>> = RefCell::new(Default::default());
}

pub(crate) struct Actors;

impl Actors {
    // Accesses actor by program id.

    pub(crate) fn access<R>(
        program_id: ActorId,
        access: impl FnOnce(Option<&TestActor>) -> R,
    ) -> R {
        ACTORS_STORAGE.with_borrow(|storage| access(storage.get(&program_id)))
    }

    // Modifies actor by program id.
    pub(crate) fn modify<R>(
        program_id: ActorId,
        modify: impl FnOnce(Option<&mut TestActor>) -> R,
    ) -> R {
        ACTORS_STORAGE.with_borrow_mut(|storage| modify(storage.get_mut(&program_id)))
    }

    // Inserts actor by program id.
    pub(crate) fn insert(program_id: ActorId, actor: TestActor) -> Option<TestActor> {
        ACTORS_STORAGE.with_borrow_mut(|storage| storage.insert(program_id, actor))
    }

    // Checks if actor by program id exists.
    pub(crate) fn contains_key(program_id: ActorId) -> bool {
        ACTORS_STORAGE.with_borrow(|storage| storage.contains_key(&program_id))
    }

    // Checks if actor by program id is a user.
    pub(crate) fn is_user(id: ActorId) -> bool {
        // Non-existent program is a user
        ACTORS_STORAGE.with_borrow(|storage| storage.get(&id).is_none())
    }

    // Checks if actor by program id is active.
    pub(crate) fn is_active_program(id: ActorId) -> bool {
        ACTORS_STORAGE.with_borrow(|storage| {
            matches!(
                storage.get(&id),
                Some(TestActor::Initialized(_) | TestActor::Uninitialized(_, _))
            )
        })
    }

    // Checks if actor by program id is a program.
    pub(crate) fn is_program(id: ActorId) -> bool {
        // if it's not a user, then it's a program
        !Self::is_user(id)
    }

    // Returns all program ids.
    pub(crate) fn program_ids() -> Vec<ActorId> {
        ACTORS_STORAGE.with_borrow(|storage| storage.keys().copied().collect())
    }

    // Clears actors storage.
    pub(crate) fn clear() {
        ACTORS_STORAGE.with_borrow_mut(|storage| storage.clear())
    }
}

impl fmt::Debug for Actors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        ACTORS_STORAGE.with_borrow(|storage| f.debug_map().entries(storage.iter()).finish())
    }
}

#[derive(Debug)]
pub(crate) enum TestActor {
    Initialized(Program),
    // Contract: program is always `Some`, option is used to take ownership
    Uninitialized(Option<MessageId>, Option<Program>),
    FailedInit,
    CodeNotExists,
    Exited(ActorId),
}

impl TestActor {
    // Creates a new uninitialized actor.
    pub(crate) fn new(init_message_id: Option<MessageId>, program: Program) -> Self {
        TestActor::Uninitialized(init_message_id, Some(program))
    }

    // # Panics
    // If actor is initialized or dormant
    pub(crate) fn set_initialized(&mut self) {
        assert!(
            self.is_uninitialized(),
            "can't transmute actor, which isn't uninitialized"
        );

        if let TestActor::Uninitialized(_, maybe_prog) = self {
            *self = TestActor::Initialized(
                maybe_prog
                    .take()
                    .expect("actor storage contains only `Some` values by contract"),
            );
        }
    }

    // Checks if actor is uninitialized.
    pub(crate) fn is_uninitialized(&self) -> bool {
        matches!(self, TestActor::Uninitialized(..))
    }

    // Checks if actor is initialized.
    pub(crate) fn is_initialized(&self) -> bool {
        matches!(self, TestActor::Initialized(..))
    }

    // Returns `Some` if actor contains genuine program.
    pub(crate) fn genuine_program(&self) -> Option<&Program> {
        match self {
            TestActor::Initialized(program) | TestActor::Uninitialized(_, Some(program)) => {
                Some(program)
            }
            _ => None,
        }
    }

    // Returns `Some` if actor contains genuine program but mutable.
    pub(crate) fn genuine_program_mut(&mut self) -> Option<&mut Program> {
        match self {
            TestActor::Initialized(program) | TestActor::Uninitialized(_, Some(program)) => {
                Some(program)
            }
            _ => None,
        }
    }

    // Returns pages data of genuine program.
    pub(crate) fn get_pages_data(&self) -> Option<&BTreeMap<GearPage, PageBuf>> {
        self.genuine_program().map(|program| &program.pages_data)
    }

    // Returns pages data of genuine program but mutable.
    pub(crate) fn get_pages_data_mut(&mut self) -> Option<&mut BTreeMap<GearPage, PageBuf>> {
        self.genuine_program_mut()
            .map(|program| &mut program.pages_data)
    }

    // Gets a new executable actor derived from the inner program.
    pub(crate) fn get_executable_actor_data(
        &self,
    ) -> Option<(ExecutableActorData, InstrumentedCode)> {
        self.genuine_program().map(Program::executable_actor_data)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Program {
    pub code_id: CodeId,
    pub code: InstrumentedCode,
    pub allocations: IntervalsTree<WasmPage>,
    pub pages_data: BTreeMap<GearPage, PageBuf>,
    pub gas_reservation_map: GasReservationMap,
}

impl Program {
    pub(crate) fn executable_actor_data(&self) -> (ExecutableActorData, InstrumentedCode) {
        (
            ExecutableActorData {
                allocations: self.allocations.clone(),
                code_id: self.code_id,
                code_exports: self.code.exports().clone(),
                static_pages: self.code.static_pages(),
                gas_reservation_map: self.gas_reservation_map.clone(),
                memory_infix: Default::default(),
            },
            self.code.clone(),
        )
    }
}
