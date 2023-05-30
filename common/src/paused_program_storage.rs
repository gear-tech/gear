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

use super::{program_storage::MemoryMap, *};
use crate::storage::MapStorage;
use gear_core::{
    code::MAX_WASM_PAGE_COUNT,
    memory::{GEAR_PAGE_SIZE, WASM_PAGE_SIZE},
};
use sp_core::MAX_POSSIBLE_ALLOCATION;
use sp_io::hashing;

const SPLIT_COUNT: u16 = (WASM_PAGE_SIZE / GEAR_PAGE_SIZE) as u16 * MAX_WASM_PAGE_COUNT / 2;

// The entity helps to calculate hash of program's data and memory pages.
// Its structure designed that way to avoid memory allocation of more than MAX_POSSIBLE_ALLOCATION bytes.
struct Item {
    data: (BTreeSet<WasmPage>, H256, MemoryMap),
    remaining_pages: MemoryMap,
}

impl From<(BTreeSet<WasmPage>, H256, MemoryMap)> for Item {
    fn from(
        (allocations, code_hash, mut memory_pages): (BTreeSet<WasmPage>, H256, MemoryMap),
    ) -> Self {
        let remaining_pages = memory_pages.split_off(&GearPage::from(SPLIT_COUNT));
        Self {
            data: (allocations, code_hash, memory_pages),
            remaining_pages,
        }
    }
}

impl<BlockNumber: Copy + Saturating> From<(ActiveProgram<BlockNumber>, MemoryMap)> for Item {
    fn from((program, memory_pages): (ActiveProgram<BlockNumber>, MemoryMap)) -> Self {
        From::from((program.allocations, program.code_hash, memory_pages))
    }
}

impl Item {
    fn hash(&self) -> H256 {
        let hash = self.data.using_encoded(hashing::blake2_256);
        if self.remaining_pages.is_empty() {
            hash.into()
        } else {
            // hash the remaining memory pages prepended with the first hash
            let mut array = Vec::with_capacity(MAX_POSSIBLE_ALLOCATION as usize);
            array.extend_from_slice(&hash);
            self.remaining_pages.encode_to(&mut array);

            hashing::blake2_256(&array).into()
        }
    }
}

/// Trait to pause/resume programs.
pub trait PausedProgramStorage: super::ProgramStorage {
    type PausedProgramMap: MapStorage<Key = ProgramId, Value = (Self::BlockNumber, H256)>;

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
    ) -> Result<GasReservationMap, <Self as super::ProgramStorage>::Error> {
        let (mut program, memory_pages) = Self::remove_active_program(program_id)?;
        let gas_reservations = core::mem::take(&mut program.gas_reservation_map);

        Self::PausedProgramMap::insert(
            program_id,
            (block_number, Item::from((program, memory_pages)).hash()),
        );

        Ok(gas_reservations)
    }
}
