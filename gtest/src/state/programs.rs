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

//! Programs storage.

use crate::{BlockNumber, WasmProgram, state::WithOverlay};
use gear_common::{ActorId, GearPage, MessageId, PageBuf, storage::DoubleBTreeMap};
use gear_core::{
    pages::{WasmPage, numerated::tree::IntervalsTree},
    program::Program,
};
use std::{collections::BTreeMap, fmt, thread::LocalKey};

/// Message id used when program is set to the programs storage (with
/// [`crate::Program`]), but no message is sent yet. So for uninitialized state
/// we use a placeholder message id.
pub(crate) const PLACEHOLDER_MESSAGE_ID: MessageId =
    MessageId::new(*b"PLACEHOLDER_MESSAGE_ID\0\0\0\0\0\0\0\0\0\0");

/// Enum representing either a regular or mock WASM program in gtest.
#[derive(Debug, Clone)]
pub(crate) enum GTestProgram {
    /// Regular program execution using the default runtime.
    Default(Program<BlockNumber>),
    /// Mock program execution using custom test logic.
    Mock(MockWasmProgram),
}

impl GTestProgram {
    pub(crate) fn as_primary_program(&self) -> &Program<BlockNumber> {
        match self {
            GTestProgram::Default(program) => program,
            GTestProgram::Mock(mock) => &mock.primary,
        }
    }

    pub(crate) fn as_primary_program_mut(&mut self) -> &mut Program<BlockNumber> {
        match self {
            GTestProgram::Default(program) => program,
            GTestProgram::Mock(mock) => &mut mock.primary,
        }
    }
}

/// Structure containing mock WASM program logic and associated program data.
#[derive(Debug)]
pub(crate) struct MockWasmProgram {
    handlers: Box<dyn WasmProgram>,
    primary: Program<BlockNumber>,
}

impl MockWasmProgram {
    /// Create a new mock WASM program with the given logic and program data.
    pub(crate) fn new(handlers_impls: Box<dyn WasmProgram>, primary: Program<BlockNumber>) -> Self {
        Self {
            handlers: handlers_impls,
            primary,
        }
    }

    /// Get a mutable reference to the mock program logic.
    pub(crate) fn handlers_mut(&mut self) -> &mut dyn WasmProgram {
        &mut *self.handlers
    }
}

impl Clone for MockWasmProgram {
    fn clone(&self) -> Self {
        Self {
            handlers: self.handlers.clone_boxed(),
            primary: self.primary.clone(),
        }
    }
}

type ProgramsStorage = WithOverlay<BTreeMap<ActorId, GTestProgram>>;
type AllocationsStorage = WithOverlay<BTreeMap<ActorId, IntervalsTree<WasmPage>>>;
type MemoryPagesStorage = WithOverlay<DoubleBTreeMap<ActorId, GearPage, PageBuf>>;
thread_local! {
    static PROGRAMS_STORAGE: ProgramsStorage = WithOverlay::new(Default::default());
    static ALLOCATIONS_STORAGE: AllocationsStorage = WithOverlay::new(Default::default());
    static MEMORY_PAGES_STORAGE: MemoryPagesStorage = WithOverlay::new(Default::default());
}

fn programs_storage() -> &'static LocalKey<WithOverlay<BTreeMap<ActorId, GTestProgram>>> {
    &PROGRAMS_STORAGE
}

fn allocations_storage()
-> &'static LocalKey<WithOverlay<BTreeMap<ActorId, IntervalsTree<WasmPage>>>> {
    &ALLOCATIONS_STORAGE
}

fn memory_pages_storage()
-> &'static LocalKey<WithOverlay<DoubleBTreeMap<ActorId, GearPage, PageBuf>>> {
    &MEMORY_PAGES_STORAGE
}

pub(crate) struct ProgramsStorageManager;

impl ProgramsStorageManager {
    // Accesses underlying primary program of a [`Program`] type, which is owned
    // by [`GTestProgram`] variants.
    pub(crate) fn access_primary_program<R>(
        program_id: ActorId,
        access: impl FnOnce(Option<&Program<BlockNumber>>) -> R,
    ) -> R {
        programs_storage().with(|storage| {
            access(
                storage
                    .data()
                    .get(&program_id)
                    .map(|gtest_program| gtest_program.as_primary_program()),
            )
        })
    }

    // Inserts actor directly by program id.
    pub(crate) fn insert_program(program_id: ActorId, program: GTestProgram) -> bool {
        programs_storage().with(|storage| storage.data_mut().insert(program_id, program).is_some())
    }

    // Modifies [`GTestProgram`] by program id.
    pub(crate) fn modify_program<R>(
        program_id: ActorId,
        modify: impl FnOnce(Option<&mut GTestProgram>) -> R,
    ) -> R {
        programs_storage().with(|storage| modify(storage.data_mut().get_mut(&program_id)))
    }

    // Checks if actor by program id exists.
    pub(crate) fn has_program(program_id: ActorId) -> bool {
        programs_storage().with(|storage| storage.data().contains_key(&program_id))
    }

    // Checks if actor by program id is a user.
    pub(crate) fn is_user(id: ActorId) -> bool {
        // Non-existent program is a user
        programs_storage().with(|storage| storage.data().get(&id).is_none())
    }

    // Checks if actor by program id is active.
    pub(crate) fn is_active_program(id: ActorId) -> bool {
        programs_storage().with(|storage| {
            storage
                .data()
                .get(&id)
                .map(|gtest_program| gtest_program.as_primary_program().is_active())
                .unwrap_or(false)
        })
    }

    // Checks if actor by program id is a program.
    pub(crate) fn is_program(id: ActorId) -> bool {
        // if it's not a user, then it's a program
        !Self::is_user(id)
    }

    // Checks if the `id` belongs to a mock program.
    pub(crate) fn is_mock_program(id: ActorId) -> bool {
        programs_storage().with(|storage| {
            storage
                .data()
                .get(&id)
                .map(|gtest_program| matches!(gtest_program, GTestProgram::Mock(_)))
                .unwrap_or(false)
        })
    }

    // Returns all program ids.
    pub(crate) fn program_ids() -> Vec<ActorId> {
        programs_storage().with(|storage| storage.data().keys().copied().collect())
    }

    // Clears programs storage.
    pub(crate) fn clear() {
        programs_storage().with(|storage| storage.data_mut().clear())
    }

    pub(crate) fn allocations(program_id: ActorId) -> Option<IntervalsTree<WasmPage>> {
        allocations_storage().with(|storage| storage.data().get(&program_id).cloned())
    }

    pub(crate) fn set_allocations(program_id: ActorId, allocations: IntervalsTree<WasmPage>) {
        programs_storage().with(|storage| {
            if let Some(program) = storage.data_mut().get_mut(&program_id)
                && let Program::Active(active_program) = program.as_primary_program_mut()
            {
                active_program.allocations_tree_len = u32::try_from(allocations.intervals_amount())
                    .unwrap_or_else(|err| {
                        // This panic is impossible because page numbers are u32.
                        unreachable!("allocations tree length is too big to fit into u32: {err}")
                    });
            }
        });
        allocations_storage().with(|storage| {
            storage.data_mut().insert(program_id, allocations);
        });
    }

    pub(crate) fn program_page(program_id: ActorId, page: GearPage) -> Option<PageBuf> {
        memory_pages_storage().with(|storage| storage.data().get(&program_id, &page).cloned())
    }

    pub(crate) fn program_pages(program_id: ActorId) -> BTreeMap<GearPage, PageBuf> {
        memory_pages_storage().with(|storage| storage.data().iter_key(&program_id).collect())
    }

    pub(crate) fn set_program_page(program_id: ActorId, page: GearPage, buf: PageBuf) {
        memory_pages_storage().with(|storage| {
            storage.data_mut().insert(program_id, page, buf);
        });
    }

    pub(crate) fn remove_program_page(program_id: ActorId, page: GearPage) {
        memory_pages_storage().with(|storage| {
            storage.data_mut().remove(program_id, page);
        });
    }
}

impl fmt::Debug for ProgramsStorageManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        programs_storage().with(|storage| {
            f.debug_map()
                .entries(
                    storage
                        .data()
                        .iter()
                        .map(|(k, v)| (k, v.as_primary_program())),
                )
                .finish()
        })
    }
}
