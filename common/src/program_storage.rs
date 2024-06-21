// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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

use gear_core::pages::{numerated::tree::IntervalsTree, WasmPage};

use super::*;
use crate::storage::{MapStorage, TripleMapStorage};
use core::fmt::Debug;

/// Trait for ProgramStorage errors.
///
/// Contains constructors for all existing errors.
pub trait Error {
    /// Program already exists in storage.
    fn duplicate_item() -> Self;

    /// Program is not found in the storage.
    fn program_not_found() -> Self;

    /// Program is not an instance of ActiveProgram.
    fn not_active_program() -> Self;

    /// There is no data for specified `program_id` and `page`.
    fn cannot_find_page_data() -> Self;

    /// Failed to find the program binary code.
    fn program_code_not_found() -> Self;
}

pub type MemoryMap = BTreeMap<GearPage, PageBuf>;

/// Trait to work with program data in a storage.
pub trait ProgramStorage {
    type InternalError: Error;
    type Error: From<Self::InternalError> + Debug;
    type BlockNumber: Copy + Saturating;
    type AccountId: Eq + PartialEq;

    type ProgramMap: MapStorage<Key = ProgramId, Value = Program<Self::BlockNumber>>;
    type MemoryPageMap: TripleMapStorage<
        Key1 = ProgramId,
        Key2 = MemoryInfix,
        Key3 = GearPage,
        Value = PageBuf,
    >;
    type AllocationsMap: MapStorage<Key = ProgramId, Value = IntervalsTree<WasmPage>>;
    type PagesWithDataMap: MapStorage<Key = ProgramId, Value = IntervalsTree<GearPage>>;

    /// Attempt to remove all items from all the associated maps.
    fn reset() {
        Self::ProgramMap::clear();
        Self::MemoryPageMap::clear();
        Self::AllocationsMap::clear();
        Self::PagesWithDataMap::clear();
    }

    /// Store a program to be associated with the given key `program_id` from the map.
    fn add_program(
        program_id: ProgramId,
        program: ActiveProgram<Self::BlockNumber>,
    ) -> Result<(), Self::Error> {
        Self::ProgramMap::mutate(program_id, |maybe| {
            if maybe.is_some() {
                return Err(Self::InternalError::duplicate_item().into());
            }

            *maybe = Some(Program::Active(program));
            Ok(())
        })
    }

    /// Load the program associated with the given key `program_id` from the map.
    fn get_program(program_id: ProgramId) -> Option<Program<Self::BlockNumber>> {
        Self::ProgramMap::get(&program_id)
    }

    /// Does the program (explicitly) exist in storage?
    fn program_exists(program_id: ProgramId) -> bool {
        Self::ProgramMap::contains_key(&program_id)
    }

    /// Update the active program under the given key `program_id`.
    fn update_active_program<F, ReturnType>(
        program_id: ProgramId,
        update_action: F,
    ) -> Result<ReturnType, Self::Error>
    where
        F: FnOnce(&mut ActiveProgram<Self::BlockNumber>) -> ReturnType,
    {
        Self::update_program_if_active(program_id, |program, _bn| match program {
            Program::Active(active_program) => update_action(active_program),
            _ => unreachable!("invariant kept by update_program_if_active"),
        })
    }

    fn pages_with_data(program_id: ProgramId) -> Option<IntervalsTree<GearPage>> {
        Self::PagesWithDataMap::get(&program_id)
    }

    fn append_pages_with_data(program_id: ProgramId, pages: impl Iterator<Item = GearPage>) {
        Self::PagesWithDataMap::mutate(program_id, |maybe| {
            let tree = maybe.get_or_insert(Default::default());
            pages.for_each(|page| tree.insert(page));
        });
    }

    fn remove_pages_with_data(
        program_id: ProgramId,
        memory_infix: MemoryInfix,
        pages: impl Iterator<Item = GearPage>,
    ) {
        Self::PagesWithDataMap::mutate(program_id, |maybe| {
            let tree = maybe.get_or_insert(Default::default());
            for page in pages {
                if tree.contains(page) {
                    tree.remove(page);
                    Self::remove_program_page_data(program_id, memory_infix, page)
                }
            }
        });
    }

    fn allocations(program_id: ProgramId) -> Option<IntervalsTree<WasmPage>> {
        Self::AllocationsMap::get(&program_id)
    }

    fn set_allocations(program_id: ProgramId, allocations: IntervalsTree<WasmPage>) {
        Self::update_active_program(program_id, |program| {
            program.allocations_tree_len = u32::try_from(allocations.intervals_amount())
                .unwrap_or_else(|err| {
                    // This panic is impossible because page numbers are u32.
                    unreachable!("allocations tree length is too big to fit into u32: {err}")
                });
        })
        .unwrap_or_else(|err| {
            // Set allocations can be called only for active programs.
            unreachable!("Failed to update program allocations: {err:?}")
        });
        Self::AllocationsMap::insert(program_id, allocations);
    }

    fn memory_infix(program_id: ProgramId) -> Option<MemoryInfix> {
        match Self::ProgramMap::get(&program_id) {
            Some(Program::Active(program)) => Some(program.memory_infix),
            _ => None,
        }
    }

    /// Update the program under the given key `program_id` only if the
    /// stored program is an active one.
    fn update_program_if_active<F, ReturnType>(
        program_id: ProgramId,
        update_action: F,
    ) -> Result<ReturnType, Self::Error>
    where
        F: FnOnce(&mut Program<Self::BlockNumber>, Self::BlockNumber) -> ReturnType,
    {
        let mut program =
            Self::ProgramMap::get(&program_id).ok_or(Self::InternalError::program_not_found())?;
        let bn = match program {
            Program::Active(ref p) => p.expiration_block,
            _ => return Err(Self::InternalError::not_active_program().into()),
        };

        let result = update_action(&mut program, bn);
        Self::ProgramMap::insert(program_id, program);

        Ok(result)
    }

    /// Return program data for each page from `pages`.
    fn get_program_data_for_pages(
        program_id: ProgramId,
        memory_infix: MemoryInfix,
        pages: impl Iterator<Item = GearPage>,
    ) -> Result<MemoryMap, Self::Error> {
        let mut pages_data = BTreeMap::new();
        for page in pages {
            let data = Self::MemoryPageMap::get(&program_id, &memory_infix, &page)
                .ok_or(Self::InternalError::cannot_find_page_data())?;
            pages_data.insert(page, data);
        }

        Ok(pages_data)
    }

    /// Store a memory page buffer to be associated with the given keys `program_id`, `memory_infix` and `page` from the map.
    fn set_program_page_data(
        program_id: ProgramId,
        memory_infix: MemoryInfix,
        page: GearPage,
        page_buf: PageBuf,
    ) {
        Self::MemoryPageMap::insert(program_id, memory_infix, page, page_buf);
    }

    /// Remove a memory page buffer under the given keys `program_id`, `memory_infix` and `page`.
    fn remove_program_page_data(
        program_id: ProgramId,
        memory_infix: MemoryInfix,
        page_num: GearPage,
    ) {
        Self::MemoryPageMap::remove(program_id, memory_infix, page_num);
    }

    /// Remove all memory page buffers under the given keys `program_id` and `memory_infix`.
    fn remove_program_pages(program_id: ProgramId, memory_infix: MemoryInfix) {
        Self::MemoryPageMap::clear_prefix(program_id, memory_infix);
    }

    /// Final full prefix that prefixes all keys of memory pages.
    fn pages_final_prefix() -> [u8; 32];
}
