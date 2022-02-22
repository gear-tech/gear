// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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
use frame_support::storage::PrefixIterator;

#[derive(Clone, Debug, Decode, Encode, PartialEq, TypeInfo)]
struct PausedProgram {
    program_id: H256,
    program: ActiveProgram,
    pages_hash: H256,
    wait_list: Vec<Dispatch>,
    waiting_init: Vec<H256>,
}

fn decode_dispatch_tuple(_: &[u8], value: &[u8]) -> Result<(Dispatch, u32), codec::Error> {
    <(Dispatch, u32)>::decode(&mut &*value)
}

fn memory_pages_hash(pages: &BTreeMap<u32, Vec<u8>>) -> H256 {
    pages.using_encoded(sp_io::hashing::blake2_256).into()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Error
{
    ProgramNotFound,
    ProgramTerminated,
}

pub fn pause_program(program_id: H256) -> Result<(), Error> {
    let program = get_program(program_id).ok_or(Error::ProgramNotFound)?;
    let program: ActiveProgram = program.try_into().map_err(|_| Error::ProgramTerminated)?;

    let prefix = wait_prefix(program_id);
    let previous_key = prefix.clone();

    let paused_program = PausedProgram {
        program_id,
        pages_hash: memory_pages_hash(&get_program_pages(program_id, program.persistent_pages.clone())
            .expect("pause_program: active program exists, therefore pages do")),
        program,
        wait_list: PrefixIterator::<_, ()>::new(prefix, previous_key, decode_dispatch_tuple)
            .drain()
            .map(|(d, _)| d)
            .collect(),
        waiting_init: waiting_init_take_messages(program_id),
    };

    // code shouldn't be removed
    // remove_program(program_id);
    sp_io::storage::clear_prefix(&pages_prefix(program_id), None);
    sp_io::storage::clear_prefix(&program_key(program_id), None);

    sp_io::storage::set(&paused_program_key(program_id), &paused_program.encode());

    Ok(())
}

fn paused_program_key(id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PAUSED_PROGRAM_PREFIX);
    id.encode_to(&mut key);
    key
}

pub fn paused_program_exists(id: H256) -> bool {
    sp_io::storage::exists(&paused_program_key(id))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResumeError
{
    ProgramNotFound,
    WrongMemoryPages,
}

pub fn resume_program(program_id: H256, memory_pages: BTreeMap<u32, Vec<u8>>, block_number: u32) -> Result<(), ResumeError> {
    let paused_program_key = &paused_program_key(program_id);
    let paused_program = sp_io::storage::get(paused_program_key)
        .map(|bytes| PausedProgram::decode(&mut &bytes[..]).expect("resume_program: encoded correctly"))
        .ok_or(ResumeError::ProgramNotFound)?;

    if paused_program.pages_hash != memory_pages_hash(&memory_pages) {
        return Err(ResumeError::WrongMemoryPages);
    }

    sp_io::storage::clear_prefix(paused_program_key, None);

    set_program(program_id, paused_program.program, memory_pages);

    paused_program.wait_list.into_iter().for_each(|m| insert_waiting_message(program_id, m.message.id, m, block_number));
    sp_io::storage::set(&waiting_init_prefix(program_id), &paused_program.waiting_init.encode()[..]);

    Ok(())
}
