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

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
struct Item {
    allocations: BTreeSet<WasmPage>,
    memory_pages: BTreeMap<GearPage, PageBuf>,
    code_hash: H256,
    state: ProgramState,
}

impl From<(ActiveProgram, BTreeMap<GearPage, PageBuf>)> for Item {
    fn from((program, memory_pages): (ActiveProgram, BTreeMap<GearPage, PageBuf>)) -> Self {
        Self {
            allocations: program.allocations,
            memory_pages,
            code_hash: program.code_hash,
            state: program.state,
        }
    }
}

impl Item {
    fn hash(&self) -> H256 {
        self.using_encoded(sp_io::hashing::blake2_256).into()
    }
}

/// Trait to pause/resume programs.
pub trait PausedProgramStorage {
    type BlockNumber;

    type PausedProgramMap: MapStorage<Key = ProgramId, Value = (Self::BlockNumber, H256)>;

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

            Self::PausedProgramMap::insert(
                program_id,
                (block_number, Item::from((program, memory_pages)).hash()),
            );

            gas_reservations
        })
    }
}
