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
        pages_hash: get_program_pages(program_id, program.persistent_pages.clone())
            .expect("pause_program: active program exists, therefore pages do")
            .using_encoded(sp_io::hashing::blake2_256)
            .into(),
        program,
        wait_list: PrefixIterator::<_, ()>::new(prefix, previous_key, decode_dispatch_tuple)
            .drain()
            .map(|(d, _)| d)
            .collect(),
        waiting_init: waiting_init_take_messages(program_id),
    };

    // code shouldn't be removed
    // remove_program(program_id);
    let mut pages_prefix = STORAGE_PROGRAM_PAGES_PREFIX.to_vec();
    let program_key = &program_key(program_id);
    pages_prefix.extend(program_key);
    sp_io::storage::clear_prefix(&pages_prefix, None);
    sp_io::storage::clear_prefix(program_key, None);

    sp_io::storage::set(&paused_program_key(program_id), &paused_program.encode());

    Ok(())
}

pub fn paused_program_key(id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_PAUSED_PROGRAM_PREFIX);
    id.encode_to(&mut key);
    key
}

pub fn paused_program_exists(id: H256) -> bool {
    sp_io::storage::exists(&paused_program_key(id))
}
