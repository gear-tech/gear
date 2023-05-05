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
struct Item {
    allocations: BTreeSet<WasmPage>,
    memory_pages: MemoryMap,
    code_hash: H256,
}

impl From<(BTreeSet<WasmPage>, H256, MemoryMap)> for Item {
    fn from((allocations, code_hash, memory_pages): (BTreeSet<WasmPage>, H256, MemoryMap)) -> Self {
        Self {
            allocations,
            memory_pages,
            code_hash,
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
        self.using_encoded(sp_io::hashing::blake2_256).into()
    }
}

/// Successfull result of calling `resume_program`.
/// Both variants contain block number when `resume_program`
/// was called the first time.
pub enum ResumeResult<BlockNumber> {
    /// Program resumed successfully.
    Ok(BlockNumber),
    /// Provided data is incomplete or incorrect. The data
    /// saved to the storage so a caller is able to call `resume_program`
    /// again with remaining data.
    IncompleteData(BlockNumber),
}

/// Trait to pause/resume programs.
pub trait PausedProgramStorage: super::ProgramStorage {
    type PausedProgramMap: MapStorage<Key = ProgramId, Value = (Self::BlockNumber, H256)>;
    type ResumePageMap: MapStorage<Key = ProgramId, Value = (Self::BlockNumber, BTreeSet<GearPage>)>;
    type CodeStorage: super::CodeStorage;

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
    ) -> Result<GasReservationMap, Self::Error> {
        let (mut program, memory_pages) = Self::remove_active_program(program_id)?;
        let gas_reservations = core::mem::take(&mut program.gas_reservation_map);

        Self::PausedProgramMap::insert(
            program_id,
            (block_number, Item::from((program, memory_pages)).hash()),
        );

        Ok(gas_reservations)
    }

    /// Resume program with the given key `program_id`.
    fn resume_program(
        program_id: ProgramId,
        allocations: BTreeSet<WasmPage>,
        mut memory_pages: MemoryMap,
        code_hash: H256,
        expiration_block: Self::BlockNumber,
        current_block: Self::BlockNumber,
    ) -> Result<ResumeResult<Self::BlockNumber>, Self::Error> {
        let Some((_block_number, hash)) = Self::PausedProgramMap::get(&program_id) else {
            return Err(Self::InternalError::item_not_found().into());
        };

        let (block, uploaded_pages) = match Self::ResumePageMap::get(&program_id) {
            Some((block, uploaded_pages)) => (block, uploaded_pages),
            None => (current_block, Default::default()),
        };

        // at first upload new memory pages (they could overwrite the old ones)
        for (page, page_buf) in memory_pages.iter() {
            Self::set_program_page_data(program_id, *page, page_buf.clone());
        }

        // then load remaining memory pages
        let pages = uploaded_pages
            .iter()
            .filter(|k| !memory_pages.contains_key(*k));
        let mut pages_data = Self::get_program_data_for_pages(program_id, pages)?;
        memory_pages.append(&mut pages_data);

        // and check hash
        let uploaded_pages = memory_pages.keys().copied().collect();
        let current_hash = Item::from((allocations.clone(), code_hash, memory_pages)).hash();
        if current_hash != hash {
            Self::ResumePageMap::insert(program_id, (block, uploaded_pages));

            return Ok(ResumeResult::IncompleteData(block));
        }

        let code =
            Self::CodeStorage::get_code(CodeId::from_origin(code_hash)).ok_or_else(|| {
                log::debug!("resume_program: code {code_hash} not found");

                Self::InternalError::item_not_found()
            })?;
        let program = ActiveProgram {
            allocations,
            pages_with_data: uploaded_pages,
            gas_reservation_map: Default::default(),
            code_hash,
            code_exports: code.exports().clone(),
            static_pages: code.static_pages(),
            state: ProgramState::Initialized,
            expiration_block,
        };

        Self::PausedProgramMap::remove(program_id);
        Self::ResumePageMap::remove(program_id);

        Self::add_program(program_id, program)
            .expect("invariant kept by the PausedProgramStorage trait");

        Ok(ResumeResult::Ok(block))
    }

    /// Remove all data created by a call to `resume_program`.
    fn remove_resume_data(program_id: ProgramId) -> Result<(), Self::Error> {
        if !Self::PausedProgramMap::contains_key(&program_id) {
            return Err(Self::InternalError::item_not_found().into());
        }

        Self::ResumePageMap::remove(program_id);
        Self::remove_program_pages(program_id);

        Ok(())
    }
}
