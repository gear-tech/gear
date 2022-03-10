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
use common::{self, QueuedDispatch};
use frame_support::storage::PrefixIterator;
use scale_info::TypeInfo;
use sp_std::collections::btree_map::BTreeMap;

#[derive(Clone, Debug, PartialEq, Decode, Encode, TypeInfo)]
pub(super) struct PausedProgram {
    program_id: H256,
    program: common::ActiveProgram,
    pages_hash: H256,
    wait_list: Vec<QueuedDispatch>,
    waiting_init: Vec<H256>,
}

fn decode_dispatch_tuple(_key: &[u8], value: &[u8]) -> Result<(QueuedDispatch, u32), codec::Error> {
    <(QueuedDispatch, u32)>::decode(&mut &*value)
}

fn memory_pages_hash(pages: &BTreeMap<u32, Vec<u8>>) -> H256 {
    pages.using_encoded(sp_io::hashing::blake2_256).into()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PauseError {
    ProgramNotFound,
    ProgramTerminated,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResumeError {
    ProgramNotFound,
    WrongMemoryPages,
}

impl<T: Config> pallet::Pallet<T> {
    pub fn pause_program(program_id: H256) -> Result<(), PauseError> {
        let program = common::get_program(program_id).ok_or(PauseError::ProgramNotFound)?;
        let program: common::ActiveProgram = program
            .try_into()
            .map_err(|_| PauseError::ProgramTerminated)?;

        let prefix = common::wait_prefix(program_id);
        let previous_key = prefix.clone();

        let paused_program = PausedProgram {
            program_id,
            pages_hash: memory_pages_hash(
                &common::get_program_pages(program_id, program.persistent_pages.clone())
                    .expect("pause_program: active program exists, therefore pages do"),
            ),
            program,
            wait_list: PrefixIterator::<_, ()>::new(prefix, previous_key, decode_dispatch_tuple)
                .drain()
                .map(|(d, _)| d)
                .collect(),
            waiting_init: common::waiting_init_take_messages(program_id),
        };

        // code shouldn't be removed
        // remove_program(program_id);
        sp_io::storage::clear_prefix(&common::pages_prefix(program_id), None);
        sp_io::storage::clear_prefix(&common::program_key(program_id), None);

        PausedPrograms::<T>::insert(program_id, paused_program);

        Ok(())
    }

    pub fn paused_program_exists(id: H256) -> bool {
        PausedPrograms::<T>::contains_key(id)
    }

    pub fn resume_program(
        program_id: H256,
        memory_pages: BTreeMap<u32, Vec<u8>>,
        block_number: u32,
    ) -> Result<(), ResumeError> {
        let paused_program =
            PausedPrograms::<T>::get(program_id).ok_or(ResumeError::ProgramNotFound)?;

        if paused_program.pages_hash != memory_pages_hash(&memory_pages) {
            return Err(ResumeError::WrongMemoryPages);
        }

        PausedPrograms::<T>::remove(program_id);

        common::set_program(program_id, paused_program.program, memory_pages);

        paused_program.wait_list.into_iter().for_each(|m| {
            common::insert_waiting_message(program_id, m.message.id, m, block_number)
        });
        sp_io::storage::set(
            &common::waiting_init_prefix(program_id),
            &paused_program.waiting_init.encode()[..],
        );

        Ok(())
    }
}
