// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use super::*;
use crate::storage::{AppendMapStorage, DoubleMapStorage, MapStorage};
use core::fmt::Debug;

/// Trait for ProgramStorage errors.
///
/// Contains constructors for all existing errors.
pub trait Error {
    /// Program already exists in storage.
    fn duplicate_item() -> Self;

    /// Program is not found in the storage.
    fn item_not_found() -> Self;

    /// Program is not an instance of ActiveProgram.
    fn not_active_program() -> Self;

    /// There is no data for specified `program_id` and `page`.
    fn cannot_find_page_data() -> Self;
}

/// Trait to work with program data in a storage.
pub trait ProgramStorage {
    type InternalError: Error;
    type Error: From<Self::InternalError> + Debug;
    type BlockNumber;

    type ProgramMap: MapStorage<Key = ProgramId, Value = (Program, Self::BlockNumber)>;
    type MemoryPageMap: DoubleMapStorage<Key1 = ProgramId, Key2 = GearPage, Value = PageBuf>;
    type WaitingInitMap: AppendMapStorage<MessageId, ProgramId, Vec<MessageId>>;

    /// Attempt to remove all items from all the associated maps.
    fn reset() {
        Self::ProgramMap::clear();
        Self::MemoryPageMap::clear();
        Self::WaitingInitMap::clear();
    }

    /// Store a program to be associated with the given key `program_id` from the map.
    fn add_program(
        program_id: ProgramId,
        program: ActiveProgram,
        block_number: Self::BlockNumber,
    ) -> Result<(), Self::Error> {
        Self::ProgramMap::mutate(program_id, |maybe| {
            if maybe.is_some() {
                return Err(Self::InternalError::duplicate_item().into());
            }

            *maybe = Some((Program::Active(program), block_number));
            Ok(())
        })
    }

    /// Load the program associated with the given key `program_id` from the map.
    fn get_program(program_id: ProgramId) -> Option<(Program, Self::BlockNumber)> {
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
        F: FnOnce(&mut ActiveProgram, &mut Self::BlockNumber) -> ReturnType,
    {
        Self::update_program_if_active(program_id, |program, bn_ref| match program {
            Program::Active(active_program) => update_action(active_program, bn_ref),
            _ => unreachable!("invariant kept by update_program_if_active"),
        })
    }

    /// Update the program under the given key `program_id` only if the
    /// stored program is an active one.
    fn update_program_if_active<F, ReturnType>(
        program_id: ProgramId,
        update_action: F,
    ) -> Result<ReturnType, Self::Error>
    where
        F: FnOnce(&mut Program, &mut Self::BlockNumber) -> ReturnType,
    {
        let (mut program, mut bn) =
            Self::ProgramMap::get(&program_id).ok_or(Self::InternalError::item_not_found())?;
        match program {
            Program::Active(_) => (),
            _ => return Err(Self::InternalError::not_active_program().into()),
        }

        let result = update_action(&mut program, &mut bn);
        Self::ProgramMap::insert(program_id, (program, bn));

        Ok(result)
    }

    /// Return program data for each page from `pages`.
    fn get_program_data_for_pages<'a>(
        program_id: ProgramId,
        pages: impl Iterator<Item = &'a GearPage>,
    ) -> Result<BTreeMap<GearPage, PageBuf>, Self::Error> {
        let mut pages_data = BTreeMap::new();
        for page in pages {
            let data = Self::MemoryPageMap::get(&program_id, page)
                .ok_or(Self::InternalError::cannot_find_page_data())?;
            pages_data.insert(*page, data);
        }

        Ok(pages_data)
    }

    /// Store a memory page buffer to be associated with the given keys `program_id` and `page` from the map.
    fn set_program_page_data(program_id: ProgramId, page: GearPage, page_buf: PageBuf) {
        Self::MemoryPageMap::insert(program_id, page, page_buf);
    }

    /// Remove a memory page buffer under the given keys `program_id` and `page`.
    fn remove_program_page_data(program_id: ProgramId, page_num: GearPage) {
        Self::MemoryPageMap::remove(program_id, page_num);
    }

    /// Remove all memory page buffers under the given key `program_id`.
    fn remove_program_pages(program_id: ProgramId) {
        Self::MemoryPageMap::clear_prefix(program_id);
    }

    /// Final full prefix that prefixes all keys of memory pages.
    fn pages_final_prefix() -> [u8; 32];

    /// Load the messages to uninitialized program associated with the given key `program_id` from the map.
    fn waiting_init_get_messages(program_id: ProgramId) -> Vec<MessageId> {
        Self::WaitingInitMap::get(&program_id).unwrap_or_default()
    }

    /// Take the messages to uninitialized program under the given key `program_id`.
    fn waiting_init_take_messages(program_id: ProgramId) -> Vec<MessageId> {
        Self::WaitingInitMap::take(program_id).unwrap_or_default()
    }

    /// Append the given message id to the list of messages to uninitialized program in the storage.
    fn waiting_init_append_message_id(dest_prog_id: ProgramId, message_id: MessageId) {
        Self::WaitingInitMap::append(dest_prog_id, message_id);
    }
}
