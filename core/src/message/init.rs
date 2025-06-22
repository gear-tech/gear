// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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

use crate::{
    buffer::Payload,
    ids::{prelude::*, ActorId, CodeId, MessageId},
    message::{
        Dispatch, DispatchKind, GasLimit, Message, Packet, Salt, StoredDispatch, StoredMessage,
        Value,
    },
};

/// Message for Init entry point.
/// Used to initiate a newly created program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitMessage {
    /// Message id.
    id: MessageId,
    /// Message destination.
    destination: ActorId,
    /// Message payload.
    payload: Payload,
    /// Message optional gas limit.
    gas_limit: Option<GasLimit>,
    /// Message value.
    value: Value,
}

impl InitMessage {
    /// Create InitMessage from InitPacket.
    pub fn from_packet(id: MessageId, packet: InitPacket) -> Self {
        Self {
            id,
            destination: packet.program_id,
            payload: packet.payload,
            gas_limit: packet.gas_limit,
            value: packet.value,
        }
    }

    /// Convert InitMessage into Message.
    pub fn into_message(self, source: ActorId) -> Message {
        Message::new(
            self.id,
            source,
            self.destination,
            self.payload,
            self.gas_limit,
            self.value,
            None,
        )
    }

    /// Convert InitMessage into StoredMessage.
    pub fn into_stored(self, source: ActorId) -> StoredMessage {
        self.into_message(source).into_stored()
    }

    /// Convert InitMessage into Dispatch.
    pub fn into_dispatch(self, source: ActorId) -> Dispatch {
        Dispatch::new(DispatchKind::Init, self.into_message(source))
    }

    /// Convert InitMessage into StoredDispatch.
    pub fn into_stored_dispatch(self, source: ActorId) -> StoredDispatch {
        self.into_dispatch(source).into_stored()
    }

    /// Message id.
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Message destination.
    pub fn destination(&self) -> ActorId {
        self.destination
    }

    /// Message payload bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.inner()
    }

    /// Message optional gas limit.
    pub fn gas_limit(&self) -> Option<GasLimit> {
        self.gas_limit
    }

    /// Message value.
    pub fn value(&self) -> Value {
        self.value
    }
}

// TODO: #issue 3320
/// Init message packet.
///
/// This structure is preparation for future init message sending. Has no message id.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(Default))]
pub struct InitPacket {
    /// Newly created program id.
    program_id: ActorId,
    /// Code id.
    code_id: CodeId,
    /// Salt.
    salt: Salt,
    /// Message payload.
    payload: Payload,
    /// Message optional gas limit.
    gas_limit: Option<GasLimit>,
    /// Message value.
    value: Value,
}

impl InitPacket {
    /// Create new InitPacket via user.
    pub fn new_from_user(
        code_id: CodeId,
        salt: Salt,
        payload: Payload,
        gas_limit: GasLimit,
        value: Value,
    ) -> Self {
        Self {
            program_id: ActorId::generate_from_user(code_id, salt.inner()),
            code_id,
            salt,
            payload,
            gas_limit: Some(gas_limit),
            value,
        }
    }

    /// Create new InitPacket via program.
    pub fn new_from_program(
        code_id: CodeId,
        salt: Salt,
        payload: Payload,
        message_id: MessageId,
        gas_limit: Option<GasLimit>,
        value: Value,
    ) -> Self {
        Self {
            program_id: ActorId::generate_from_program(message_id, code_id, salt.inner()),
            code_id,
            salt,
            payload,
            gas_limit,
            value,
        }
    }

    /// Packet destination (newly created program id).
    pub fn destination(&self) -> ActorId {
        self.program_id
    }

    /// Code id.
    pub fn code_id(&self) -> CodeId {
        self.code_id
    }

    /// Salt.
    pub fn salt(&self) -> &[u8] {
        self.salt.inner()
    }
}

impl Packet for InitPacket {
    fn payload_bytes(&self) -> &[u8] {
        self.payload.inner()
    }

    fn payload_len(&self) -> u32 {
        self.payload.len_u32()
    }

    fn gas_limit(&self) -> Option<GasLimit> {
        self.gas_limit
    }

    fn value(&self) -> Value {
        self.value
    }

    fn kind() -> DispatchKind {
        DispatchKind::Init
    }
}
