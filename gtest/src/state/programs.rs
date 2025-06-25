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
use gear_common::{
    auxiliary::{BlockNumber, DoubleBTreeMap},
    ActiveProgram, ActorId, CodeId, GearPage, MessageId, PageBuf,
};
use gear_core::{
    code::InstrumentedCode,
    pages::{numerated::tree::IntervalsTree, WasmPage},
    program::Program,
    reservation::GasReservationMap,
};
use std::{cell::RefCell, collections::BTreeMap, fmt};

/// Message id used when program is set to the programs storage (with
/// [`crate::Program`]), but no message is sent yet. So for uninitialized state
/// we use a placeholder message id.
pub(crate) const PLACEHOLDER_MESSAGE_ID: MessageId = MessageId::new(PLACEHOLDER_MESSAGE_ID_BYTES);
const PLACEHOLDER_MESSAGE_ID_BYTES: [u8; 32] = [
    80, 76, 65, 67, 69, 72, 79, 76, 68, 69, 82, 95, 77, 69, 83, 83, 65, 71, 69, 95, 73, 68, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0,
];
const _: () = {
    let expected = b"PLACEHOLDER_MESSAGE_ID\0\0\0\0\0\0\0\0\0\0";
    let mut i = 0;
    while i < 32 {
        assert!(PLACEHOLDER_MESSAGE_ID_BYTES[i] == expected[i]);
        i += 1;
    }
};

thread_local! {
    static PROGRAMS_STORAGE: RefCell<BTreeMap<ActorId, Program<BlockNumber>>> = RefCell::new(Default::default());
    static ALLOCATIONS_STORAGE: RefCell<BTreeMap<ActorId, IntervalsTree<WasmPage>>> = RefCell::new(Default::default());
    static MEMORY_PAGES_STORAGE: RefCell<DoubleBTreeMap<ActorId, GearPage, PageBuf>> = RefCell::new(Default::default());
}

pub(crate) struct ProgramsStorageManager;

impl ProgramsStorageManager {
    // Accesses actor by program id.
    pub(crate) fn access_program<R>(
        program_id: ActorId,
        access: impl FnOnce(Option<&Program<BlockNumber>>) -> R,
    ) -> R {
        PROGRAMS_STORAGE.with_borrow(|storage| access(storage.get(&program_id)))
    }

    // Modifies actor by program id.
    pub(crate) fn modify_program<R>(
        program_id: ActorId,
        modify: impl FnOnce(Option<&mut Program<BlockNumber>>) -> R,
    ) -> R {
        PROGRAMS_STORAGE.with_borrow_mut(|storage| modify(storage.get_mut(&program_id)))
    }

    // Inserts actor by program id.
    pub(crate) fn insert_program(
        program_id: ActorId,
        actor: Program<BlockNumber>,
    ) -> Option<Program<BlockNumber>> {
        PROGRAMS_STORAGE.with_borrow_mut(|storage| storage.insert(program_id, actor))
    }

    // Checks if actor by program id exists.
    pub(crate) fn has_program(program_id: ActorId) -> bool {
        PROGRAMS_STORAGE.with_borrow(|storage| storage.contains_key(&program_id))
    }

    // Checks if actor by program id is a user.
    pub(crate) fn is_user(id: ActorId) -> bool {
        // Non-existent program is a user
        PROGRAMS_STORAGE.with_borrow(|storage| storage.get(&id).is_none())
    }

    // Checks if actor by program id is active.
    pub(crate) fn is_active_program(id: ActorId) -> bool {
        PROGRAMS_STORAGE.with_borrow(|storage| {
            storage
                .get(&id)
                .map(|program| program.is_active())
                .unwrap_or(false)
        })
    }

    // Checks if actor by program id is a program.
    pub(crate) fn is_program(id: ActorId) -> bool {
        // if it's not a user, then it's a program
        !Self::is_user(id)
    }

    // Returns all program ids.
    pub(crate) fn program_ids() -> Vec<ActorId> {
        PROGRAMS_STORAGE.with_borrow(|storage| storage.keys().copied().collect())
    }

    // Clears programs storage.
    pub(crate) fn clear() {
        PROGRAMS_STORAGE.with_borrow_mut(|storage| storage.clear())
    }

    pub(crate) fn allocations(program_id: ActorId) -> Option<IntervalsTree<WasmPage>> {
        ALLOCATIONS_STORAGE.with_borrow(|storage| storage.get(&program_id).cloned())
    }

    pub(crate) fn set_allocations(program_id: ActorId, allocations: IntervalsTree<WasmPage>) {
        ALLOCATIONS_STORAGE.with_borrow_mut(|storage| {
            storage.insert(program_id, allocations);
        });
    }

    pub(crate) fn program_page(program_id: ActorId, page: GearPage) -> Option<PageBuf> {
        MEMORY_PAGES_STORAGE.with_borrow(|storage| storage.get(&program_id, &page).cloned())
    }

    pub(crate) fn set_program_page(program_id: ActorId, page: GearPage, buf: PageBuf) {
        MEMORY_PAGES_STORAGE.with_borrow_mut(|storage| {
            storage.insert(program_id, page, buf);
        });
    }

    pub(crate) fn remove_program_page(program_id: ActorId, page: GearPage) {
        MEMORY_PAGES_STORAGE.with_borrow_mut(|storage| {
            storage.remove(program_id, page);
        });
    }
}

impl fmt::Debug for ProgramsStorageManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        PROGRAMS_STORAGE.with_borrow(|storage| f.debug_map().entries(storage.iter()).finish())
    }
}
