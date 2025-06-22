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
use gear_common::{auxiliary::BlockNumber, ActorId, CodeId, GearPage, MessageId, PageBuf};
use gear_core::{
    code::InstrumentedCode,
    pages::{numerated::tree::IntervalsTree, WasmPage},
    reservation::GasReservationMap,
    program::Program
};

thread_local! {
    static ACTORS_STORAGE: RefCell<BTreeMap<ActorId, Program<BlockNumber>>> = RefCell::new(Default::default());
}

pub(crate) struct Actors;

impl Actors {
    // Accesses actor by program id.

    pub(crate) fn access<R>(
        program_id: ActorId,
        access: impl FnOnce(Option<&Program<BlockNumber>>) -> R,
    ) -> R {
        ACTORS_STORAGE.with_borrow(|storage| access(storage.get(&program_id)))
    }

    // Modifies actor by program id.
    pub(crate) fn modify<R>(
        program_id: ActorId,
        modify: impl FnOnce(Option<&mut Program<BlockNumber>>) -> R,
    ) -> R {
        ACTORS_STORAGE.with_borrow_mut(|storage| modify(storage.get_mut(&program_id)))
    }

    // Inserts actor by program id.
    pub(crate) fn insert(program_id: ActorId, actor: Program<BlockNumber>) -> Option<Program<BlockNumber>> {
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
                Some(Program::Initialized(_) | Program::Uninitialized(_, _))
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
