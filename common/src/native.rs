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

use gear_core::{
    message::Message,
    program::{ProgramId, Program},
};

use sp_core::H256;

pub fn queue_message(message: Message) {
    let message = crate::Message {
        source: H256::from_slice(&message.source.as_slice()),
        dest: H256::from_slice(&message.dest.as_slice()),
        payload: message.payload.into_raw(),
        gas_limit: message.gas_limit,
        value: message.value,
    };

    crate::queue_message(message)
}

pub fn dequeue_message() -> Option<Message> {
    crate::dequeue_message()
        .map(|msg| {
            Message {
                source: ProgramId::from_slice(&msg.source[..]),
                dest: ProgramId::from_slice(&msg.dest[..]),
                payload: msg.payload.into(),
                gas_limit: msg.gas_limit,
                value: msg.value,
            }
        })
}

pub fn get_program(id: ProgramId) -> Option<Program> {
    crate::get_program(H256::from_slice(id.as_slice()))
        .map(|prog|
            Program::new(
                id,
                prog.code,
                prog.static_pages,
            )
        )
}

pub fn set_program(program: Program) {
    crate::set_program(
        H256::from_slice(program.id().as_slice()),
        crate::Program {
            static_pages: program.static_pages().to_vec(),
            code: program.code().to_vec(),
        }
    );
}

pub fn remove_program(id: ProgramId) {
    crate::remove_program(H256::from_slice(id.as_slice()));
}

pub fn page_info(page: u32) -> Option<ProgramId> {
    crate::page_info(page).map(|pid| ProgramId::from_slice(&pid[..]))
}

pub fn alloc(page: u32, pid: ProgramId) {
    crate::alloc(page, H256::from_slice(pid.as_slice()));
}

pub fn dealloc(page: u32) {
    crate::dealloc(page);
}
