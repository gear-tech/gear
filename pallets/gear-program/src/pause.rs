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
use codec::{Decode, Encode};
use common::Origin as _;
use frame_support::{dispatch::DispatchResult, storage::PrefixIterator};
use gear_core::{
    memory::{PageBuf, PageNumber},
    message::StoredDispatch,
};
use scale_info::TypeInfo;

#[derive(Clone, Debug, PartialEq, Decode, Encode, TypeInfo)]
pub(super) struct PausedProgram {
    program_id: H256,
    program: common::ActiveProgram,
    pages_hash: H256,
    wait_list_hash: H256,
    waiting_init: Vec<H256>,
}

fn decode_dispatch_tuple(_key: &[u8], value: &[u8]) -> Result<(StoredDispatch, u32), codec::Error> {
    <(StoredDispatch, u32)>::decode(&mut &*value)
}

fn memory_pages_hash(pages: &BTreeMap<PageNumber, PageBuf>) -> H256 {
    pages.using_encoded(sp_io::hashing::blake2_256).into()
}

fn wait_list_hash(wait_list: &BTreeMap<H256, StoredDispatch>) -> H256 {
    wait_list.using_encoded(sp_io::hashing::blake2_256).into()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PauseError {
    ProgramNotFound,
    ProgramTerminated,
    InvalidPageDataSize,
}

impl<T: Config> pallet::Pallet<T> {
    pub fn pause_program(program_id: H256) -> Result<(), PauseError> {
        let program = common::get_program(program_id).ok_or(PauseError::ProgramNotFound)?;
        let program: common::ActiveProgram = program
            .try_into()
            .map_err(|_| PauseError::ProgramTerminated)?;

        let prefix = common::wait_prefix(program_id);
        let previous_key = prefix.clone();

        let pages_data = common::get_program_pages_data(program_id, &program).map_err(|e| {
            log::error!("{}", e);
            PauseError::InvalidPageDataSize
        })?;

        let paused_program = PausedProgram {
            program_id,
            pages_hash: memory_pages_hash(&pages_data),
            program,
            wait_list_hash: wait_list_hash(
                &PrefixIterator::<_, ()>::new(prefix, previous_key, decode_dispatch_tuple)
                    .drain()
                    .map(|(d, _)| (d.id().into_origin(), d))
                    .collect(),
            ),
            waiting_init: common::waiting_init_take_messages(program_id),
        };

        // code shouldn't be removed
        // remove_program(program_id);
        sp_io::storage::clear_prefix(&common::pages_prefix(program_id), None);
        sp_io::storage::clear_prefix(&common::program_key(program_id), None);

        PausedPrograms::<T>::insert(program_id, paused_program);

        Self::deposit_event(Event::ProgramPaused(program_id));

        Ok(())
    }

    pub fn program_paused(id: H256) -> bool {
        PausedPrograms::<T>::contains_key(id)
    }

    pub(super) fn resume_program_impl(
        program_id: H256,
        memory_pages: BTreeMap<PageNumber, PageBuf>,
        wait_list: BTreeMap<H256, StoredDispatch>,
        block_number: u32,
    ) -> DispatchResult {
        let paused_program =
            PausedPrograms::<T>::get(program_id).ok_or(Error::<T>::PausedProgramNotFound)?;

        if paused_program.pages_hash != memory_pages_hash(&memory_pages) {
            return Err(Error::<T>::WrongMemoryPages.into());
        }

        if paused_program.wait_list_hash != wait_list_hash(&wait_list) {
            return Err(Error::<T>::WrongWaitList.into());
        }

        PausedPrograms::<T>::remove(program_id);

        if let Err(err) =
            common::set_program_and_pages_data(program_id, paused_program.program, memory_pages)
        {
            log::error!("{}", err);
            return Err(Error::<T>::NotAllocatedPageWithData.into());
        }

        wait_list.into_iter().for_each(|(msg_id, d)| {
            common::insert_waiting_message(program_id, msg_id, d, block_number)
        });
        sp_io::storage::set(
            &common::waiting_init_prefix(program_id),
            &paused_program.waiting_init.encode()[..],
        );

        Ok(())
    }
}
