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

#[derive(Clone, Copy, Debug)]
pub enum Error {
    /// Program already exists in storage.
    DuplicateItem,
    /// Program is not found in the storage.
    ItemNotFound,
    /// Program is not an instance of ActiveProgram.
    NotActiveProgram,
    /// There is no data for specified `program_id` and `page`.
    CannotFindDataForPage {
        program_id: ProgramId,
        page: PageNumber,
    },
}

/// Trait to work with program data in a storage.
pub trait ProgramStorage {
    type ProgramMap: MapStorage<Key = ProgramId, Value = Program>;
    type MemoryPageMap: DoubleMapStorage<Key1 = ProgramId, Key2 = PageNumber, Value = PageBuf>;
    type WaitingInitMap: AppendMapStorage<MessageId, ProgramId, Vec<MessageId>>;

    fn reset() {
        Self::ProgramMap::clear();
        Self::MemoryPageMap::clear();
        Self::WaitingInitMap::clear();
    }

    fn add_program(program_id: ProgramId, program: ActiveProgram) -> Result<(), Error> {
        Self::ProgramMap::mutate(program_id, |maybe| {
            if maybe.is_some() {
                return Err(Error::DuplicateItem);
            }

            *maybe = Some(Program::Active(program));
            Ok(())
        })
    }

    fn get_program(program_id: ProgramId) -> Option<Program> {
        Self::ProgramMap::get(&program_id)
    }

    fn program_exists(program_id: ProgramId) -> bool {
        Self::ProgramMap::contains_key(&program_id)
    }

    fn update_active_program<F, ReturnType>(
        program_id: ProgramId,
        update_action: F,
    ) -> Result<ReturnType, Error>
    where
        F: FnOnce(&mut ActiveProgram) -> ReturnType,
    {
        Self::update_program_if_active(program_id, |program| match program {
            Program::Active(active_program) => update_action(active_program),
            _ => unreachable!("invariant kept by update_program_if_active"),
        })
    }

    fn update_program_if_active<F, ReturnType>(
        program_id: ProgramId,
        update_action: F,
    ) -> Result<ReturnType, Error>
    where
        F: FnOnce(&mut Program) -> ReturnType,
    {
        let mut program = Self::ProgramMap::get(&program_id).ok_or(Error::ItemNotFound)?;
        match program {
            Program::Active(_) => (),
            _ => return Err(Error::NotActiveProgram),
        }

        let result = update_action(&mut program);
        Self::ProgramMap::insert(program_id, program);

        Ok(result)
    }

    fn get_program_data_for_pages<'a>(
        program_id: ProgramId,
        pages: impl Iterator<Item = &'a PageNumber>,
    ) -> Result<BTreeMap<PageNumber, PageBuf>, Error> {
        let mut pages_data = BTreeMap::new();
        for page in pages {
            let data = Self::MemoryPageMap::get(&program_id, page).ok_or(
                Error::CannotFindDataForPage {
                    program_id,
                    page: *page,
                },
            )?;
            pages_data.insert(*page, data);
        }

        Ok(pages_data)
    }

    fn set_program_page_data(program_id: ProgramId, page: PageNumber, page_buf: PageBuf) {
        Self::MemoryPageMap::insert(program_id, page, page_buf);
    }

    fn remove_program_page_data(program_id: ProgramId, page_num: PageNumber) {
        Self::MemoryPageMap::remove(program_id, page_num);
    }

    fn remove_program_pages(program_id: ProgramId) {
        Self::MemoryPageMap::clear_prefix(program_id);
    }

    fn pages_final_prefix() -> [u8; 32];

    fn waiting_init_get_messages(program_id: ProgramId) -> Vec<MessageId> {
        Self::WaitingInitMap::get(&program_id).unwrap_or_default()
    }

    fn waiting_init_take_messages(program_id: ProgramId) -> Vec<MessageId> {
        Self::WaitingInitMap::take(program_id).unwrap_or_default()
    }

    fn waiting_init_append_message_id(dest_prog_id: ProgramId, message_id: MessageId) {
        Self::WaitingInitMap::append(dest_prog_id, message_id);
    }
}
