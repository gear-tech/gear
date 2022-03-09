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

use crate::identifiers::{CodeId, MessageId, ProgramId};
use crate::message::{Dispatch, DispatchKind, GasLimit, Message, Payload, Salt, Value};
use codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Init message.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct InitMessage {
    /// Message id.
    id: MessageId,
    /// Message destination.
    destination: ProgramId,
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
            destination: packet.new_program_id,
            payload: packet.payload,
            gas_limit: packet.gas_limit,
            value: packet.value,
        }
    }

    /// Convert InitMessage into Message.
    pub fn into_message(self, source: ProgramId) -> Message {
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

    /// Convert InitMessage into Dispatch.
    pub fn into_dispatch(self, source: ProgramId) -> Dispatch {
        Dispatch::new(DispatchKind::Init, self.into_message(source))
    }

    /// Message id.
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Message destination.
    pub fn destination(&self) -> ProgramId {
        self.destination
    }

    /// Message payload reference.
    pub fn payload(&self) -> &[u8] {
        self.payload.as_ref()
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

/// Init message packet.
///
/// This structure is preparation for future init message sending. Has no message id.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct InitPacket {
    /// Newly created program id.
    new_program_id: ProgramId,
    /// Program code id.
    code_id: CodeId,
    /// Salt for program id creation.
    salt: Salt,
    /// Message payload.
    payload: Payload,
    /// Message optional gas limit.
    gas_limit: Option<GasLimit>,
    /// Message value.
    value: Value,
}

impl InitPacket {
    /// Create new InitPacket.
    pub fn new(
        code_id: CodeId,
        salt: Salt,
        payload: Payload,
        gas_limit: Option<GasLimit>,
        value: Value,
    ) -> Self {
        let new_program_id = ProgramId::generate(code_id, &salt);
        Self {
            new_program_id,
            code_id,
            salt,
            payload,
            gas_limit,
            value,
        }
    }

    /// Packet destination (newly created program id).
    pub fn destination(&self) -> ProgramId {
        self.new_program_id
    }

    /// Packet program code id.
    pub fn code_id(&self) -> CodeId {
        self.code_id
    }

    /// Packet salt for program id creation.
    pub fn salt(&self) -> &[u8] {
        self.salt.as_ref()
    }

    /// Packet payload reference.
    pub fn payload(&self) -> &[u8] {
        self.payload.as_ref()
    }

    /// Packet optional gas limit.
    pub fn gas_limit(&self) -> Option<GasLimit> {
        self.gas_limit
    }

    /// Packet value.
    pub fn value(&self) -> Value {
        self.value
    }
}
