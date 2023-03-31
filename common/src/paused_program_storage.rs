// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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
use crate::storage::MapStorage;
use core::fmt::Debug;

/// Trait for ProgramStorage errors.
///
/// Contains constructors for all existing errors.
pub trait Error {
    // / Program already exists in storage.
    // fn duplicate_item() -> Self;

    // / Program is not found in the storage.
    // fn item_not_found() -> Self;

    // / Program is not an instance of ActiveProgram.
    // fn not_active_program() -> Self;

    // / There is no data for specified `program_id` and `page`.
    // fn cannot_find_page_data() -> Self;
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
pub struct PausedProgram<BlockNumber> {
    program: ActiveProgram,
    pages_hash: H256,
    block_number: BlockNumber,
}

fn memory_pages_hash(pages: &BTreeMap<GearPage, PageBuf>) -> H256 {
    pages.using_encoded(sp_io::hashing::blake2_256).into()
}

/// Trait to pause/resume programs.
pub trait PausedProgramStorage {
    type InternalError: Error;
    type Error: From<Self::InternalError> + Debug;
    type BlockNumber;

    type PausedProgramMap: MapStorage<Key = ProgramId, Value = PausedProgram<Self::BlockNumber>>;

    type ProgramStorage: super::ProgramStorage<BlockNumber = Self::BlockNumber>;

    /// Attempt to remove all items from all the associated maps.
    fn reset() {
        Self::PausedProgramMap::clear();
    }

    /// Does the paused program (explicitly) exist in storage?
    fn paused_program_exists(program_id: &ProgramId) -> bool {
        Self::PausedProgramMap::contains_key(program_id)
    }

    /// Pause an active program with the given key `program_id`.
    ///
    /// Return corresponding map with gas reservations if the program was paused.
    fn pause_program(
        program_id: ProgramId,
        block_number: Self::BlockNumber,
    ) -> Option<GasReservationMap> {
        Self::ProgramStorage::remove_active_program(program_id).map(|mut program| {
            let gas_reservations = program.gas_reservation_map.clone();
            program.gas_reservation_map.clear();

            let _messages = Self::ProgramStorage::waiting_init_take_messages(program_id);
            let memory_pages = Self::ProgramStorage::get_program_data_for_pages(
                program_id,
                program.pages_with_data.iter(),
            )
            .expect("active program has pages with data");
            Self::ProgramStorage::remove_program_pages(program_id);

            let pages_hash = memory_pages_hash(&memory_pages);
            Self::PausedProgramMap::insert(
                program_id,
                PausedProgram {
                    program,
                    pages_hash,
                    block_number,
                },
            );

            gas_reservations
        })
    }
}
