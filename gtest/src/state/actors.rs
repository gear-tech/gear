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

use core_processor::common::ExecutableActorData;
use gear_common::{auxiliary::overlay::WithOverlay, ActorId, CodeId, GearPage, MessageId, PageBuf};
use gear_core::{
    code::InstrumentedCode,
    pages::{numerated::tree::IntervalsTree, WasmPage},
    reservation::GasReservationMap,
};
use std::{collections::BTreeMap, fmt};

thread_local! {
    pub(super) static ACTORS_STORAGE: WithOverlay<BTreeMap<ActorId, TestActor>> = Default::default();
}

fn storage() -> &'static std::thread::LocalKey<WithOverlay<BTreeMap<ActorId, TestActor>>> {
    &ACTORS_STORAGE
}

pub(crate) struct Actors;

impl Actors {
    // Accesses actor by program id.
    pub(crate) fn access<R>(
        program_id: ActorId,
        access: impl FnOnce(Option<&TestActor>) -> R,
    ) -> R {
        storage().with(|storage| access(storage.data().get(&program_id)))
    }

    // Modifies actor by program id.
    pub(crate) fn modify<R>(
        program_id: ActorId,
        modify: impl FnOnce(Option<&mut TestActor>) -> R,
    ) -> R {
        storage().with(|storage| modify(storage.data_mut().get_mut(&program_id)))
    }

    // Inserts actor by program id.
    pub(crate) fn insert(program_id: ActorId, actor: TestActor) -> Option<TestActor> {
        storage().with(|storage| storage.data_mut().insert(program_id, actor))
    }

    // Checks if actor by program id exists.
    pub(crate) fn contains_key(program_id: ActorId) -> bool {
        storage().with(|storage| storage.data().contains_key(&program_id))
    }

    // Checks if actor by program id is a user.
    pub(crate) fn is_user(id: ActorId) -> bool {
        // Non-existent program is a user
        storage().with(|storage| storage.data().get(&id).is_none())
    }

    // Checks if actor by program id is active.
    pub(crate) fn is_active_program(id: ActorId) -> bool {
        storage().with(|storage| {
            matches!(
                storage.data().get(&id),
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
        storage().with(|storage| storage.data().keys().copied().collect())
    }

    // Clears actors storage.
    pub(crate) fn clear() {
        storage().with(|storage| storage.data_mut().clear())
    }
}

impl fmt::Debug for Actors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        storage().with(|storage| f.debug_map().entries(storage.data().iter()).finish())
    }
}

#[derive(Debug, Clone)]
pub(crate) enum TestActor {
    Initialized(Program),
    // Contract: program is always `Some`, option is used to take ownership
    Uninitialized(Option<MessageId>, Option<Program>),
    FailedInit,
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
    pub(crate) fn program(&self) -> Option<&Program> {
        match self {
            TestActor::Initialized(program) | TestActor::Uninitialized(_, Some(program)) => {
                Some(program)
            }
            _ => None,
        }
    }

    // Returns `Some` if actor contains genuine program but mutable.
    pub(crate) fn program_mut(&mut self) -> Option<&mut Program> {
        match self {
            TestActor::Initialized(program) | TestActor::Uninitialized(_, Some(program)) => {
                Some(program)
            }
            _ => None,
        }
    }

    // Returns pages data of genuine program.
    pub(crate) fn pages(&self) -> Option<&BTreeMap<GearPage, PageBuf>> {
        self.program().map(|program| &program.pages_data)
    }

    // Returns pages data of genuine program but mutable.
    pub(crate) fn pages_mut(&mut self) -> Option<&mut BTreeMap<GearPage, PageBuf>> {
        self.program_mut().map(|program| &mut program.pages_data)
    }

    // Gets a new executable actor derived from the inner program.
    pub(crate) fn executable_actor_data(&self) -> Option<(ExecutableActorData, InstrumentedCode)> {
        self.program().map(Program::executable_actor_data)
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
