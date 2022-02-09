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
struct ExitedProgram {
    program_id: H256,
    program: Program,
    pages_hash: H256,
    wait_list: Vec<Message>,
}

fn decode_message_tuple(_: &[u8], value: &[u8]) -> Result<(Message, u32), codec::Error> {
    <(Message, u32)>::decode(&mut &*value)
}

#[derive(Debug)]
pub struct ProgramNotFound;

pub fn exit_program(program_id: H256) -> Result<(), ProgramNotFound> {
    let program = get_program(program_id).ok_or(ProgramNotFound)?;

    let prefix = wait_prefix(program_id);
    let previous_key = prefix.clone();

    let exited_program = ExitedProgram {
        program_id,
        pages_hash: get_program_pages(program_id, program.persistent_pages.clone())
            .using_encoded(sp_io::hashing::blake2_256)
            .into(),
        program,
        wait_list: PrefixIterator::<_, ()>::new(prefix, previous_key, decode_message_tuple)
            .drain()
            .map(|(m, _)| m)
            .collect(),
    };

    remove_program(program_id);

    sp_io::storage::set(&exited_program_key(program_id), &exited_program.encode());

    Ok(())
}

pub(super) fn exited_program_key(id: H256) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend(STORAGE_EXITED_PROGRAM_PREFIX);
    id.encode_to(&mut key);
    key
}

pub fn exited_program_exists(id: H256) -> bool {
    sp_io::storage::exists(&exited_program_key(id))
}
