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

use primitive_types::H256;
use sp_std::vec::Vec;

use crate::{Message, Origin};

impl Origin for MessageId {
    fn into_origin(self) -> H256 {
        let mut bytes = [0; 32];
        bytes.copy_from_slice(self.as_slice());
        H256(bytes)
    }

    fn from_origin(val: H256) -> Self {
        Self::from_slice(val.as_ref())
    }
}

impl Origin for ProgramId {
    fn into_origin(self) -> H256 {
        let mut bytes = [0; 32];
        bytes.copy_from_slice(self.as_slice());
        H256(bytes)
    }

    fn from_origin(val: H256) -> Self {
        Self::from_slice(val.as_ref())
    }
}

impl From<CoreMessage> for Message {
    fn from(message: CoreMessage) -> Self {
        Self {
            id: message.id.into_origin(),
            source: message.source.into_origin(),
            dest: message.dest.into_origin(),
            payload: message.payload.into_raw(),
            gas_limit: message.gas_limit,
            value: message.value,
            reply: message
                .reply
                .map(|(message_id, exit_code)| (message_id.into_origin(), exit_code)),
        }
    }
}

impl From<Message> for CoreMessage {
    fn from(message: Message) -> Self {
        Self {
            id: MessageId::from_origin(message.id),
            source: ProgramId::from_origin(message.source),
            dest: ProgramId::from_origin(message.dest),
            payload: message.payload.into(),
            gas_limit: message.gas_limit,
            value: message.value,
            reply: message
                .reply
                .map(|(message_id, exit_code)| (MessageId::from_origin(message_id), exit_code)),
        }
    }
}

pub fn queue_message(message: CoreMessage) {
    crate::queue_message(message.into())
}

pub fn dequeue_message() -> Option<CoreMessage> {
    crate::dequeue_message().map(Into::into)
}

pub fn get_program(id: ProgramId) -> Option<Program> {
    crate::get_program(id.into_origin()).map(|prog| {
        let persistent_pages = crate::get_program_pages(id.into_origin(), prog.persistent_pages);
        if let Some(code) = crate::get_code(prog.code_hash) {
            let mut program = Program::new(id, code, persistent_pages).unwrap();
            program.set_message_nonce(prog.nonce);
            program
        } else {
            Program::new(id, Vec::new(), persistent_pages).unwrap()
        }
    })
}

pub fn set_program(program: Program) {
    let code_hash = sp_io::hashing::blake2_256(program.code()).into();
    // This code is only used in tests and is redundant for
    // production. TODO to be fixed in #524
    if !crate::code_exists(code_hash) {
        crate::set_code(code_hash, program.code());
    }
    crate::set_program(
        H256::from_slice(program.id().as_slice()),
        crate::Program {
            static_pages: program.static_pages(),
            persistent_pages: program
                .get_pages()
                .iter()
                .map(|(num, _)| num.raw())
                .collect(),
            code_hash,
            nonce: program.message_nonce(),
        },
        program
            .get_pages()
            .iter()
            .map(|(num, buf)| (num.raw(), buf.to_vec()))
            .collect(),
    );
}

pub fn remove_program(id: ProgramId) {
    crate::remove_program(H256::from_slice(id.as_slice()));
}

pub fn program_exists(id: ProgramId) -> bool {
    crate::program_exists(H256::from_slice(id.as_slice()))
}

pub fn insert_waiting_message(
    prog_id: ProgramId,
    msg_id: MessageId,
    message: CoreMessage,
    bn: u32,
) {
    crate::insert_waiting_message(
        H256::from_slice(prog_id.as_slice()),
        H256::from_slice(msg_id.as_slice()),
        message.into(),
        bn,
    );
}

pub fn remove_waiting_message(prog_id: ProgramId, msg_id: MessageId) -> Option<(CoreMessage, u32)> {
    crate::remove_waiting_message(
        H256::from_slice(prog_id.as_slice()),
        H256::from_slice(msg_id.as_slice()),
    )
    .map(|(msg, bn)| (msg.into(), bn))
}
