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

use crate::{FromCoreId, FromHashId};

impl From<CoreMessage> for crate::Message {
    fn from(message: CoreMessage) -> crate::Message {
        let mut message = message;
        crate::Message {
            id: H256::from_message_id(&message.id()),
            source: H256::from_program_id(&message.source()),
            dest: H256::from_program_id(&message.dest()),
            payload: message.drain_payload().collect(),
            gas_limit: message.gas_limit(),
            value: message.value(),
            reply: message
                .reply()
                .map(|(message_id, exit_code)| (H256::from_message_id(&message_id), exit_code)),
        }
    }
}

impl From<crate::Message> for CoreMessage {
    fn from(message: crate::Message) -> CoreMessage {
        CoreMessage::from_parts(
            MessageId::from_hash_id(&message.id),
            message.payload,
            message.gas_limit,
            message.value,
        )
        .with_source(ProgramId::from_hash_id(&message.source))
        .with_dest(ProgramId::from_hash_id(&message.dest))
        .with_reply(
            message
                .reply
                .map(|(message_id, exit_code)| (MessageId::from_hash_id(&message_id), exit_code)),
        )
    }
}

pub fn queue_message(message: CoreMessage) {
    crate::queue_message(message.into())
}

pub fn dequeue_message() -> Option<CoreMessage> {
    crate::dequeue_message().map(crate::Message::into)
}

pub fn get_program(id: ProgramId) -> Option<Program> {
    let hash_id = H256::from_program_id(&id);
    crate::get_program(hash_id).map(|prog| {
        let persistent_pages = crate::get_program_pages(hash_id, prog.persistent_pages);
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
    crate::set_code(code_hash, program.code());
    crate::set_program(
        H256::from_program_id(&program.id()),
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
    crate::remove_program(H256::from_program_id(&id));
}

pub fn program_exists(id: ProgramId) -> bool {
    crate::program_exists(H256::from_program_id(&id))
}

pub fn insert_waiting_message(prog_id: ProgramId, msg_id: MessageId, message: CoreMessage) {
    crate::insert_waiting_message(
        H256::from_program_id(&prog_id),
        H256::from_message_id(&msg_id),
        message.into(),
    );
}

pub fn get_waiting_message(prog_id: ProgramId, msg_id: MessageId) -> Option<CoreMessage> {
    crate::get_waiting_message(
        H256::from_program_id(&prog_id),
        H256::from_message_id(&msg_id),
    )
    .map(|msg| msg.into())
}

pub fn remove_waiting_message(prog_id: ProgramId, msg_id: MessageId) -> Option<CoreMessage> {
    crate::remove_waiting_message(
        H256::from_program_id(&prog_id),
        H256::from_message_id(&msg_id),
    )
    .map(|msg| msg.into())
}
