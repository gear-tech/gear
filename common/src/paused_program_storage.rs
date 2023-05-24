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
use crate::storage::{AppendMapStorage, MapStorage, ValueStorage};
use core::fmt::Debug;
use sp_arithmetic::traits::UniqueSaturatedInto;
use sp_io::hashing;
use sp_runtime::AccountId32;

#[derive(Clone, Debug, Encode)]
#[codec(crate = codec)]
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
        self.using_encoded(hashing::blake2_256).into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub struct ResumeSession<BlockNumber> {
    page_count: u32,
    user: AccountId32,
    program_id: ProgramId,
    allocations: BTreeSet<WasmPage>,
    code_hash: H256,
    start_block: BlockNumber,
    rent_blocks: BlockNumber,
    rent_fee: u128,
}

pub struct ResumeResult<BlockNumber> {
    pub start_block: BlockNumber,
    pub rent_fee: u128,
    pub info: Option<(ProgramId, BlockNumber)>,
}

/// Trait to pause/resume programs.
pub trait PausedProgramStorage: super::ProgramStorage {
    type PausedProgramMap: MapStorage<Key = ProgramId, Value = (Self::BlockNumber, H256)>;
    type ResumePageMap: MapStorage<Key = ProgramId, Value = (Self::BlockNumber, BTreeSet<GearPage>)>;
    type CodeStorage: super::CodeStorage;
    type NonceStorage: ValueStorage<Value = u128>;
    type ResumeSessions: MapStorage<Key = u64, Value = ResumeSession<Self::BlockNumber>>;
    type SessionMemoryPages: AppendMapStorage<(GearPage, PageBuf), u64, Vec<(GearPage, PageBuf)>>;

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

    /// Create a session for program resume. Returns the session id on success.
    fn start_program_resume(
        user: AccountId32,
        start_block: Self::BlockNumber,
        program_id: ProgramId,
        allocations: BTreeSet<WasmPage>,
        code_hash: H256,
        rent_blocks: Self::BlockNumber,
        rent_fee: u128,
    ) -> Option<u64> {
        if !Self::paused_program_exists(&program_id) {
            return None;
        }

        let nonce = Self::NonceStorage::mutate(|nonce| match nonce {
            Some(n) => {
                let result = *n;
                *n = n.wrapping_add(1);

                result
            }
            None => {
                *nonce = Some(1);

                0
            }
        });

        let session_id = {
            let start_block: u128 = start_block.unique_saturated_into();
            u64::from_le_bytes((program_id, start_block, nonce).using_encoded(hashing::twox_64))
        };
        Self::ResumeSessions::mutate(session_id, |session| {
            if session.is_some() {
                return None;
            }

            *session = Some(ResumeSession {
                page_count: 0,
                user,
                program_id,
                allocations,
                code_hash,
                start_block,
                rent_blocks,
                rent_fee,
            });

            Some(session_id)
        })
    }

    /// Get the count of uploaded memory pages of the specified session.
    fn resume_session_page_count(session_id: &u64) -> Option<u32> {
        Self::ResumeSessions::get(session_id).map(|session| session.page_count)
    }

    /// Append program memory pages to the session data.
    fn resume_session_append(
        session_id: u64,
        user: AccountId32,
        memory_pages: Vec<(GearPage, PageBuf)>,
    ) -> Result<(), Self::Error> {
        Self::ResumeSessions::mutate(session_id, |maybe_session| {
            let Some(session) = maybe_session.as_mut() else {
                return Err(Self::InternalError::resume_session_not_found().into())
            };

            if session.user != user {
                return Err(Self::InternalError::not_session_owner().into());
            }

            session.page_count += memory_pages.len() as u32;
            for page in memory_pages {
                Self::SessionMemoryPages::append(session_id, page);
            }

            Ok(())
        })
    }

    /// Finish program resume session with the given key `session_id`.
    fn resume_session_finish(
        session_id: u64,
        user: AccountId32,
        current_block: Self::BlockNumber,
    ) -> Result<ResumeResult<Self::BlockNumber>, Self::Error> {
        Self::ResumeSessions::mutate(session_id, |maybe_session| {
            let session = match maybe_session.as_mut() {
                None => return Err(Self::InternalError::resume_session_not_found().into()),
                Some(s) if s.user != user => {
                    return Err(Self::InternalError::not_session_owner().into())
                }
                Some(s) => s,
            };

            let Some((_block_number, hash)) = Self::PausedProgramMap::get(&session.program_id) else {
                let result = ResumeResult {
                    start_block: session.start_block,
                    rent_fee: session.rent_fee,
                    info: None,
                };

                // it means that the program has been already resumed within another session
                Self::SessionMemoryPages::remove(session_id);
                *maybe_session = None;

                return Ok(result)
            };

            let memory_pages: MemoryMap = Self::SessionMemoryPages::get(&session_id)
                .unwrap_or_default()
                .into_iter()
                .collect();
            let code_hash = session.code_hash;
            let item = Item::from((session.allocations.clone(), code_hash, memory_pages));
            if item.hash() != hash {
                return Err(Self::InternalError::resume_session_failed().into());
            }

            let code =
                Self::CodeStorage::get_code(CodeId::from_origin(code_hash)).ok_or_else(|| {
                    log::debug!("Failed to find the code {code_hash} to resume program");

                    Self::InternalError::program_code_not_found()
                })?;
            let program = ActiveProgram {
                allocations: item.allocations,
                pages_with_data: item.memory_pages.keys().copied().collect(),
                gas_reservation_map: Default::default(),
                code_hash,
                code_exports: code.exports().clone(),
                static_pages: code.static_pages(),
                state: ProgramState::Initialized,
                expiration_block: current_block.saturating_add(session.rent_blocks),
            };

            let program_id = session.program_id;
            let result = ResumeResult {
                start_block: session.start_block,
                rent_fee: session.rent_fee,
                info: Some((program_id, program.expiration_block)),
            };

            // wipe all uploaded data out
            *maybe_session = None;
            Self::PausedProgramMap::remove(program_id);
            Self::SessionMemoryPages::remove(session_id);

            // set program memory pages
            for (page, page_buf) in item.memory_pages {
                Self::set_program_page_data(program_id, page, page_buf);
            }
            // and finally start the program
            Self::ProgramMap::insert(program_id, Program::Active(program));

            Ok(result)
        })
    }

    /// Remove all data created by a call to `start_program_resume`.
    fn remove_resume_session(session_id: u64) -> Result<(AccountId32, u128), Self::Error> {
        Self::ResumeSessions::mutate(session_id, |maybe_session| match maybe_session.take() {
            Some(s) => {
                Self::SessionMemoryPages::remove(session_id);

                Ok((s.user, s.rent_fee))
            }
            None => Err(Self::InternalError::item_not_found().into()),
        })
    }
}
