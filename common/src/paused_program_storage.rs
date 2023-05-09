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
use core::fmt::Debug;

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub struct Item {
    hash: H256,
    page_hashes: BTreeMap<GearPage, H256>,
}

impl From<(BTreeSet<WasmPage>, H256, MemoryMap)> for Item {
    fn from((allocations, code_hash, memory_pages): (BTreeSet<WasmPage>, H256, MemoryMap)) -> Self {
        Self {
            hash: (allocations, code_hash)
                .using_encoded(sp_io::hashing::blake2_256)
                .into(),
            page_hashes: memory_pages
                .into_iter()
                .map(|(i, page)| (i, page.using_encoded(sp_io::hashing::blake2_256).into()))
                .collect(),
        }
    }
}

/// Trait to pause/resume programs.
pub trait PausedProgramStorage: super::ProgramStorage {
    type PausedProgramMap: MapStorage<Key = ProgramId, Value = Item>;

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
    ) -> Result<GasReservationMap, <Self as super::ProgramStorage>::Error> {
        let (mut program, memory_pages) = Self::remove_active_program(program_id)?;
        let gas_reservations = core::mem::take(&mut program.gas_reservation_map);

        Self::PausedProgramMap::insert(
            program_id,
            Item::from((program.allocations, program.code_hash, memory_pages)),
        );

        Ok(gas_reservations)
    }
}
