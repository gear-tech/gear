// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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
use crate::storage::{MapStorage, ValueStorage};
use gear_core::{code::MAX_WASM_PAGES_AMOUNT, program::ProgramState};
use sp_core::MAX_POSSIBLE_ALLOCATION;
use sp_io::hashing;

pub type SessionId = u32;

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
        // TODO: consider memory pages batch size #1381
        let split_page = WasmPage::from(MAX_WASM_PAGES_AMOUNT / 2);
        let remaining_pages = memory_pages.split_off(&split_page.to_page());
        Self {
            data: (allocations, code_hash, memory_pages),
            remaining_pages,
        }
    }
}

impl<BlockNumber: Copy + Saturating> From<(ActiveProgram<BlockNumber>, MemoryMap)> for Item {
    fn from((program, memory_pages): (ActiveProgram<BlockNumber>, MemoryMap)) -> Self {
        From::from((
            program.allocations.points_iter().collect(),
            program.code_hash,
            memory_pages,
        ))
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

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub struct ResumeSession<AccountId, BlockNumber> {
    page_count: u32,
    user: AccountId,
    program_id: ProgramId,
    allocations: BTreeSet<WasmPage>,
    pages_with_data: BTreeSet<GearPage>,
    code_hash: CodeId,
    end_block: BlockNumber,
}

/// The entity defines result of the [`PausedProgramStorage::resume_session_commit()`] method.
pub struct ResumeResult<BlockNumber> {
    /// The session end block number.
    pub end_block: BlockNumber,
    /// If a program resumed successfully then this field contains
    /// a tuple with id and expiration block of the program.
    pub info: Option<(ProgramId, BlockNumber)>,
}

/// Trait to pause/resume programs.
pub trait PausedProgramStorage: super::ProgramStorage {
    type PausedProgramMap: MapStorage<Key = ProgramId, Value = (Self::BlockNumber, H256)>;
    type CodeStorage: super::CodeStorage;
    type NonceStorage: ValueStorage<Value = SessionId>;
    type ResumeSessions: MapStorage<
        Key = SessionId,
        Value = ResumeSession<Self::AccountId, Self::BlockNumber>,
    >;

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
        let (mut program, memory_pages) = Self::ProgramMap::mutate(program_id, |maybe| {
            let Some(program) = maybe.take() else {
                return Err(Self::InternalError::program_not_found().into());
            };

            let Program::Active(program) = program else {
                *maybe = Some(program);

                return Err(Self::InternalError::not_active_program().into());
            };

            let memory_pages = match Self::get_program_data_for_pages(
                program_id,
                program.memory_infix,
                program.pages_with_data.points_iter(),
            ) {
                Ok(memory_pages) => memory_pages,
                Err(e) => {
                    *maybe = Some(Program::Active(program));

                    return Err(e);
                }
            };

            Self::remove_program_pages(program_id, program.memory_infix);

            Ok((program, memory_pages))
        })?;

        let gas_reservations = core::mem::take(&mut program.gas_reservation_map);

        Self::PausedProgramMap::insert(
            program_id,
            (block_number, Item::from((program, memory_pages)).hash()),
        );

        Ok(gas_reservations)
    }

    /// Create a session for program resume. Returns the session id on success.
    fn resume_session_init(
        user: Self::AccountId,
        end_block: Self::BlockNumber,
        program_id: ProgramId,
        allocations: BTreeSet<WasmPage>,
        code_hash: CodeId,
    ) -> Result<SessionId, Self::Error> {
        if !Self::paused_program_exists(&program_id) {
            return Err(Self::InternalError::program_not_found().into());
        }

        let session_id = Self::NonceStorage::mutate(|nonce| {
            let nonce = nonce.get_or_insert(0);
            let result = *nonce;
            *nonce = result.wrapping_add(1);

            result
        });

        Self::ResumeSessions::mutate(session_id, |session| {
            if session.is_some() {
                return Err(Self::InternalError::duplicate_resume_session().into());
            }

            *session = Some(ResumeSession {
                page_count: 0,
                user,
                program_id,
                allocations,
                pages_with_data: Default::default(),
                code_hash,
                end_block,
            });

            Ok(session_id)
        })
    }

    /// Get the count of uploaded memory pages of the specified session.
    fn resume_session_page_count(session_id: &SessionId) -> Option<u32> {
        Self::ResumeSessions::get(session_id).map(|session| session.page_count)
    }

    /// Append program memory pages to the session data.
    fn resume_session_push(
        session_id: SessionId,
        user: Self::AccountId,
        memory_pages: Vec<(GearPage, PageBuf)>,
    ) -> Result<(), Self::Error> {
        // TODO: #3447 additional check

        Self::ResumeSessions::mutate(session_id, |maybe_session| {
            let session = match maybe_session.as_mut() {
                Some(s) if s.user == user => s,
                Some(_) => return Err(Self::InternalError::not_session_owner().into()),
                None => return Err(Self::InternalError::resume_session_not_found().into()),
            };

            session.page_count += memory_pages.len() as u32;
            for (page, page_buf) in memory_pages {
                session.pages_with_data.insert(page);
                Self::set_program_page_data(
                    session.program_id,
                    MemoryInfix::new(session_id),
                    page,
                    page_buf,
                );
            }

            Ok(())
        })
    }

    /// Finish program resume session with the given key `session_id`.
    fn resume_session_commit(
        session_id: SessionId,
        user: Self::AccountId,
        expiration_block: Self::BlockNumber,
    ) -> Result<ResumeResult<Self::BlockNumber>, Self::Error> {
        Self::ResumeSessions::mutate(session_id, |maybe_session| {
            let session = match maybe_session.as_mut() {
                Some(s) if s.user == user => s,
                Some(_) => return Err(Self::InternalError::not_session_owner().into()),
                None => return Err(Self::InternalError::resume_session_not_found().into()),
            };

            let Some((_block_number, hash)) = Self::PausedProgramMap::get(&session.program_id)
            else {
                let result = ResumeResult {
                    end_block: session.end_block,
                    info: None,
                };

                // it means that the program has been already resumed within another session
                Self::remove_program_pages(session.program_id, MemoryInfix::new(session_id));
                *maybe_session = None;

                return Ok(result);
            };

            if !Self::CodeStorage::exists(session.code_hash) {
                log::debug!(
                    "Failed to find the code {} to resume program",
                    session.code_hash
                );

                return Err(Self::InternalError::program_code_not_found().into());
            }

            let memory_pages = Self::get_program_data_for_pages(
                session.program_id,
                MemoryInfix::new(session_id),
                session.pages_with_data.iter().copied(),
            )
            .unwrap_or_default();
            let code_hash = session.code_hash.into_origin();
            let item = Item::from((session.allocations.clone(), code_hash, memory_pages));
            if item.hash() != hash {
                return Err(Self::InternalError::resume_session_failed().into());
            }

            let code = Self::CodeStorage::get_code(session.code_hash).unwrap_or_else(|| {
                unreachable!("Code storage corrupted: item existence checked before")
            });
            let Item {
                data: (allocations, _, memory_pages),
                remaining_pages,
            } = item;
            let program = ActiveProgram {
                allocations: allocations.into_iter().collect(),
                pages_with_data: memory_pages
                    .keys()
                    .copied()
                    .chain(remaining_pages.keys().copied())
                    .collect(),
                gas_reservation_map: Default::default(),
                code_hash,
                code_exports: code.exports().clone(),
                static_pages: code.static_pages(),
                state: ProgramState::Initialized,
                expiration_block,
                memory_infix: MemoryInfix::new(session_id),
            };

            let program_id = session.program_id;
            let result = ResumeResult {
                end_block: session.end_block,
                info: Some((program_id, program.expiration_block)),
            };

            // wipe all uploaded data out
            *maybe_session = None;
            Self::PausedProgramMap::remove(program_id);

            // and finally start the program
            Self::ProgramMap::insert(program_id, Program::Active(program));

            Ok(result)
        })
    }

    /// Remove all data created by a call to `resume_session_init`.
    fn remove_resume_session(session_id: SessionId) -> Result<(), Self::Error> {
        Self::ResumeSessions::mutate(session_id, |maybe_session| match maybe_session.take() {
            Some(session) => {
                Self::remove_program_pages(session.program_id, MemoryInfix::new(session_id));

                Ok(())
            }
            None => Err(Self::InternalError::program_not_found().into()),
        })
    }
}
