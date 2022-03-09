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

use crate::identifiers::{MessageId, ProgramId};
use crate::message::{Dispatch, DispatchKind, GasLimit, Message, Payload, Value};
use codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Handle message.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct HandleMessage {
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

impl HandleMessage {
    /// Create HandleMessage from HandlePacket.
    pub fn from_packet(id: MessageId, packet: HandlePacket) -> Self {
        Self {
            id,
            destination: packet.destination,
            payload: packet.payload,
            gas_limit: packet.gas_limit,
            value: packet.value,
        }
    }

    /// Convert HandleMessage into Message.
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

    /// Convert HandleMessage into Dispatch.
    pub fn into_dispatch(self, source: ProgramId) -> Dispatch {
        Dispatch::new(DispatchKind::Handle, self.into_message(source))
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

/// Handle message packet.
///
/// This structure is preparation for future HandleMessage sending. Has no message id.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct HandlePacket {
    /// Packet destination.
    destination: ProgramId,
    /// Packet payload.
    payload: Payload,
    /// Packet optional gas limit.
    gas_limit: Option<GasLimit>,
    /// Packet value.
    value: Value,
}

impl HandlePacket {
    /// Create new packet.
    pub fn new(
        destination: ProgramId,
        payload: Payload,
        gas_limit: Option<GasLimit>,
        value: Value,
    ) -> Self {
        Self {
            destination,
            payload,
            gas_limit,
            value,
        }
    }

    /// Packet destination.
    pub fn destination(&self) -> ProgramId {
        self.destination
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
