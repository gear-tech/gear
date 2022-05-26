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

use crate::{
    ids::{MessageId, ProgramId},
    message::{
        Dispatch, DispatchKind, ExitCode, GasLimit, Message, Packet, Payload, StoredDispatch,
        StoredMessage, Value,
    },
};
use codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Message for Reply entry point.
/// Used to create a conversation between actors.
/// [`ReplyMessage`]'s linked to the origin with [`MessageId`].
/// That allows making more abilities in business logic.
///
/// message1 = Message { id: message1_id, ... }
/// +--- reply1 = Message { id: reply1_id, ..., reply: Some((message1_id, ...)) }
/// |    `--- reply2 = Message { ..., reply: Some((reply1_id, ...)) }
/// `--- reply3 = Message { ..., reply: Some((message1_id, ...)) }
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct ReplyMessage {
    /// Message id.
    id: MessageId,
    /// Message payload.
    payload: Payload,
    /// Message optional gas limit.
    gas_limit: Option<GasLimit>,
    /// Message value.
    value: Value,
    /// Reply exit code.
    exit_code: ExitCode,
}

impl ReplyMessage {
    /// Create ReplyMessage from ReplyPacket.
    pub fn from_packet(id: MessageId, packet: ReplyPacket) -> Self {
        Self {
            id,
            payload: packet.payload,
            gas_limit: packet.gas_limit,
            value: packet.value,
            exit_code: packet.exit_code,
        }
    }

    /// Convert ReplyMessage into Message.
    pub fn into_message(
        self,
        program_id: ProgramId,
        destination: ProgramId,
        origin_msg_id: MessageId,
    ) -> Message {
        Message::new(
            self.id,
            program_id,
            destination,
            self.payload,
            self.gas_limit,
            self.value,
            Some((origin_msg_id, self.exit_code)),
        )
    }

    /// Convert ReplyMessage into StoredMessage.
    pub fn into_stored(
        self,
        program_id: ProgramId,
        destination: ProgramId,
        origin_msg_id: MessageId,
    ) -> StoredMessage {
        self.into_message(program_id, destination, origin_msg_id)
            .into()
    }

    /// Convert ReplyMessage into Dispatch.
    pub fn into_dispatch(
        self,
        source: ProgramId,
        destination: ProgramId,
        origin_msg_id: MessageId,
    ) -> Dispatch {
        Dispatch::new(
            DispatchKind::Reply,
            self.into_message(source, destination, origin_msg_id),
        )
    }

    /// Convert ReplyMessage into StoredDispatch.
    pub fn into_stored_dispatch(
        self,
        source: ProgramId,
        destination: ProgramId,
        origin_msg_id: MessageId,
    ) -> StoredDispatch {
        self.into_dispatch(source, destination, origin_msg_id)
            .into()
    }

    /// Message id.
    pub fn id(&self) -> MessageId {
        self.id
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

    /// Exit code of the reply message.
    pub fn exit_code(&self) -> ExitCode {
        self.exit_code
    }
}

/// Reply message packet.
///
/// This structure is preparation for future reply message sending. Has no message id.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct ReplyPacket {
    /// Message payload.
    payload: Payload,
    /// Message optional gas limit.
    gas_limit: Option<GasLimit>,
    /// Message value.
    value: Value,
    /// Reply exit code.
    exit_code: ExitCode,
}

impl ReplyPacket {
    /// Create new ReplyPacket without gas.
    pub fn new(payload: Payload, value: Value) -> Self {
        Self {
            payload,
            gas_limit: None,
            value,
            exit_code: 0,
        }
    }

    /// Create new ReplyPacket with gas.
    pub fn new_with_gas(payload: Payload, gas_limit: GasLimit, value: Value) -> Self {
        Self {
            payload,
            gas_limit: Some(gas_limit),
            value,
            exit_code: 0,
        }
    }

    /// Create new empty ReplyPacket (for system generation).
    pub fn system(exit_code: ExitCode) -> Self {
        Self {
            exit_code,
            ..Default::default()
        }
    }

    /// Prepend payload.
    pub(super) fn prepend(&mut self, data: Payload) {
        self.payload.splice(0..0, data);
    }

    /// Packet exit code.
    pub fn exit_code(&self) -> ExitCode {
        self.exit_code
    }
}

impl Packet for ReplyPacket {
    fn payload(&self) -> &[u8] {
        self.payload.as_ref()
    }

    fn gas_limit(&self) -> Option<GasLimit> {
        self.gas_limit
    }

    fn value(&self) -> Value {
        self.value
    }
}
