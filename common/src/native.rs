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
    message::{Message as CoreMessage, MessageId},
    program::{Program, ProgramId},
};

use sp_core::H256;

impl From<CoreMessage> for crate::Message {
    fn from(message: CoreMessage) -> crate::Message {
        crate::Message {
            id: H256::from_slice(&message.id.as_slice()),
            source: H256::from_slice(&message.source.as_slice()),
            dest: H256::from_slice(&message.dest.as_slice()),
            payload: message.payload.into_raw(),
            gas_limit: message.gas_limit,
            value: message.value,
            reply: message.reply.map(|(message_id, exit_code)| {
                (H256::from_slice(message_id.as_slice()), exit_code)
            }),
        }
    }
}

impl From<crate::Message> for CoreMessage {
    fn from(message: crate::Message) -> CoreMessage {
        CoreMessage {
            id: MessageId::from_slice(message.id.as_ref()),
            source: ProgramId::from_slice(message.id.as_ref()),
            dest: ProgramId::from_slice(message.id.as_ref()),
            payload: message.payload.into(),
            gas_limit: message.gas_limit,
            value: message.value,
            reply: message.reply.map(|(message_id, exit_code)| {
                (MessageId::from_slice(message_id.as_ref()), exit_code)
            }),
        }
    }
}

pub fn queue_message(message: CoreMessage) {
    crate::queue_message(message.into())
}

pub fn dequeue_message() -> Option<CoreMessage> {
    crate::dequeue_message().map(|msg| CoreMessage {
        id: MessageId::from_slice(&msg.id[..]),
        source: ProgramId::from_slice(&msg.source[..]),
        dest: ProgramId::from_slice(&msg.dest[..]),
        payload: msg.payload.into(),
        gas_limit: msg.gas_limit,
        value: msg.value,
        reply: msg
            .reply
            .map(|(message_id, exit_code)| (MessageId::from_slice(&message_id[..]), exit_code)),
    })
}

pub fn get_program(id: ProgramId) -> Option<Program> {
    crate::get_program(H256::from_slice(id.as_slice())).map(|prog| {
        if let Some(code) = crate::get_code(prog.code_hash) {
            let mut program = Program::new(id, code, prog.persistent_pages).unwrap();
            program.set_message_nonce(prog.nonce);
            program
        } else {
            Program::new(id, Vec::new(), prog.persistent_pages).unwrap()
        }
    })
}

pub fn set_program(program: Program) {
    let code_hash = sp_io::hashing::blake2_256(program.code()).into();
    crate::set_code(code_hash, program.code());
    crate::set_program(
        H256::from_slice(program.id().as_slice()),
        crate::Program {
            static_pages: program.static_pages(),
            persistent_pages: program
                .get_pages()
                .into_iter()
                .map(|(num, buf)| (num.raw(), buf.to_vec()))
                .collect(),
            code_hash,
            nonce: program.message_nonce(),
        },
    );
}

pub fn remove_program(id: ProgramId) {
    crate::remove_program(H256::from_slice(id.as_slice()));
}

pub fn program_exists(id: ProgramId) -> bool {
    crate::program_exists(H256::from_slice(id.as_slice()))
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

pub fn insert_waiting_message(waker_id: MessageId, message: CoreMessage) {
    crate::insert_waiting_message(H256::from_slice(waker_id.as_slice()), message.into());
}

pub fn remove_waiting_message(waker_id: MessageId) -> Option<CoreMessage> {
    crate::remove_waiting_message(H256::from_slice(waker_id.as_slice())).map(|msg| msg.into())
}
