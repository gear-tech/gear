// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
    message::{Dispatch as CoreDispatch, Message as CoreMessage, MessageId},
    program::{CodeHash, Program as CoreProgram, ProgramId},
};

use primitive_types::H256;

use crate::{Dispatch, Message, Origin};

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
            gas_limit: message.gas_limit.unwrap_or_default(),
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
            gas_limit: Some(message.gas_limit),
            value: message.value,
            reply: message
                .reply
                .map(|(message_id, exit_code)| (MessageId::from_origin(message_id), exit_code)),
        }
    }
}

impl From<Dispatch> for CoreDispatch {
    fn from(dispatch: Dispatch) -> Self {
        let Dispatch {
            kind,
            message,
            payload_store,
        } = dispatch;
        Self {
            kind,
            message: message.into(),
            payload_store,
        }
    }
}

impl From<CoreDispatch> for Dispatch {
    fn from(dispatch: CoreDispatch) -> Self {
        let CoreDispatch {
            kind,
            message,
            payload_store,
        } = dispatch;
        Self {
            kind,
            message: message.into(),
            payload_store,
        }
    }
}

pub fn set_program(program: CoreProgram) {
    let code_hash = CodeHash::generate(program.code()).into_origin();
    // This code is only used in tests and is redundant for
    // production.
    if !crate::code_exists(code_hash) {
        crate::set_code(code_hash, program.instrumented_code());
    }
    crate::set_program(
        H256::from_slice(program.id().as_slice()),
        crate::ActiveProgram {
            static_pages: program.static_pages(),
            persistent_pages: program
                .get_pages()
                .iter()
                .map(|(num, _)| num.raw())
                .collect(),
            code_hash,
            nonce: program.message_nonce(),
            state: crate::ProgramState::Initialized,
        },
        program
            .get_pages()
            .iter()
            .map(|(num, buf)| {
                let buf = buf
                    .as_ref()
                    .expect("When set program, each page must have data");
                (num.raw(), buf.to_vec())
            })
            .collect(),
    );
}
