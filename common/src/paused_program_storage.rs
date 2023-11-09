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
use crate::storage::{MapStorage, ValueStorage};
use core::{mem, num::NonZeroU16};
use gear_core::program::MemoryInfix;
use sp_io::hashing;

pub type SessionId = u32;

pub fn hash(allocations: &BTreeSet<WasmPage>, code_hash: H256, hashes: &[H256]) -> H256 {
    (allocations, code_hash, hashes)
        .using_encoded(hashing::blake2_256)
        .into()
}

pub fn batch_hash(pages: &MemoryMap) -> H256 {
    let mut data = Vec::with_capacity(sp_core::MAX_POSSIBLE_ALLOCATION as usize);
    pages.encode_to(&mut data);

    hashing::blake2_256(&data).into()
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub struct ResumeSession<AccountId, BlockNumber> {
    user: AccountId,
    program_id: ProgramId,
    allocations: BTreeSet<WasmPage>,
    pages_with_data: BTreeSet<GearPage>,
    code_hash: CodeId,
    end_block: BlockNumber,
    page_batches: Vec<PageBatch>,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum PageBatchState {
    Uploading(BTreeSet<GearPage>),
    Checked,
}

#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub struct PageBatch {
    hash: H256,
    state: PageBatchState,
}

impl From<H256> for PageBatch {
    fn from(hash: H256) -> Self {
        Self {
            hash,
            state: PageBatchState::Uploading(Default::default()),
        }
    }
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
    type BatchCapacity: Get<NonZeroU16>;

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
                program.pages_with_data.iter(),
            ) {
                Ok(memory_pages) => memory_pages,
                Err(e) => {
                    *maybe = Some(Program::Active(program));

                    return Err(e);
                }
            };

            Self::waiting_init_remove(program_id);
            Self::remove_program_pages(program_id, program.memory_infix);

            Ok((program, memory_pages))
        })?;

        let gas_reservations = mem::take(&mut program.gas_reservation_map);

        let batch_capacity = Self::BatchCapacity::get().get() as usize;
        if memory_pages.len() > batch_capacity {
            unimplemented!("todo");
        }

        let hashes = vec![batch_hash(&memory_pages)];
        let hash = hash(&program.allocations, program.code_hash, &hashes);

        Self::PausedProgramMap::insert(program_id, (block_number, hash));

        Ok(gas_reservations)
    }

    /// Create a session for program resume. Returns the session id on success.
    fn resume_session_init(
        user: Self::AccountId,
        end_block: Self::BlockNumber,
        program_id: ProgramId,
        allocations: BTreeSet<WasmPage>,
        code_hash: CodeId,
        hashes: Vec<H256>,
    ) -> Result<SessionId, Self::Error> {
        let Some((_bn, hash)) = Self::PausedProgramMap::get(&program_id) else {
            return Err(Self::InternalError::program_not_found().into());
        };

        let new_hash = self::hash(&allocations, code_hash.into_origin(), &hashes);
        if new_hash != hash {
            return Err(Self::InternalError::wrong_init_data().into());
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
                user,
                program_id,
                allocations,
                pages_with_data: Default::default(),
                code_hash,
                end_block,
                page_batches: hashes.into_iter().map(PageBatch::from).collect(),
            });

            Ok(session_id)
        })
    }

    /// Get the count of uploaded memory pages of the specified memory pages batch within the session.
    fn resume_session_batch_count(session_id: &SessionId, index: usize) -> Option<u32> {
        Self::ResumeSessions::get(session_id).and_then(|session| {
            session.page_batches.get(index).and_then(|b| match b.state {
                PageBatchState::Checked => None,
                PageBatchState::Uploading(ref p) => Some(p.len() as u32),
            })
        })
    }

    /// Append program memory pages to the session data.
    fn resume_session_push(
        session_id: SessionId,
        user: Self::AccountId,
        batch_index: usize,
        memory_pages: Vec<(GearPage, PageBuf)>,
    ) -> Result<(), Self::Error> {
        // TODO: #3447 additional check

        Self::ResumeSessions::mutate(session_id, |maybe_session| {
            let session = match maybe_session.as_mut() {
                Some(s) if s.user == user => s,
                Some(_) => return Err(Self::InternalError::not_session_owner().into()),
                None => return Err(Self::InternalError::resume_session_not_found().into()),
            };

            let pages = match session.page_batches.get_mut(batch_index) {
                Some(b) => match b.state {
                    PageBatchState::Checked => return Ok(()),
                    PageBatchState::Uploading(ref mut pages) => pages,
                },
                None => return Err(Self::InternalError::batch_not_found().into()),
            };

            for (page, page_buf) in memory_pages {
                pages.insert(page);
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

    /// Append program memory pages to the session data.
    fn resume_session_check(
        session_id: SessionId,
        user: Self::AccountId,
        batch_index: usize,
    ) -> Result<(), Self::Error> {
        Self::ResumeSessions::mutate(session_id, |maybe_session| {
            let session = match maybe_session.as_mut() {
                Some(s) if s.user == user => s,
                Some(_) => return Err(Self::InternalError::not_session_owner().into()),
                None => return Err(Self::InternalError::resume_session_not_found().into()),
            };

            let (batch_hash, page_batch) = match session.page_batches.get_mut(batch_index) {
                Some(b) => (b.hash, b),
                None => return Err(Self::InternalError::batch_not_found().into()),
            };

            let page_indexes = match page_batch.state {
                PageBatchState::Checked => return Ok(()),
                PageBatchState::Uploading(ref mut p) => p,
            };

            let pages = Self::get_program_data_for_pages(
                session.program_id,
                MemoryInfix::new(session_id),
                page_indexes.iter(),
            )?;
            if self::batch_hash(&pages) != batch_hash {
                return Err(Self::InternalError::incorrect_batch_hash().into());
            }

            session.pages_with_data.append(page_indexes);
            page_batch.state = PageBatchState::Checked;

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

            let Some((_block_number, _hash)) = Self::PausedProgramMap::get(&session.program_id)
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

            for batch in session.page_batches.iter() {
                if !matches!(batch.state, PageBatchState::Checked) {
                    return Err(Self::InternalError::resume_session_failed().into());
                }
            }

            let code_id = session.code_hash;
            let Some(code) = Self::CodeStorage::get_code(code_id) else {
                log::debug!("Failed to find the code {code_id} to resume program",);

                return Err(Self::InternalError::program_code_not_found().into());
            };
            let program = ActiveProgram {
                allocations: mem::take(&mut session.allocations),
                pages_with_data: mem::take(&mut session.pages_with_data),
                gas_reservation_map: Default::default(),
                code_hash: code_id.into_origin(),
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
