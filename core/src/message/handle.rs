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
    ids::{ActorId, MessageId},
    message::{
        Dispatch, DispatchKind, GasLimit, Message, Packet, StoredDispatch, StoredMessage, Value,
    },
};

/// Message for Handle entry point.
/// Represents a standard message that sends between actors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandleMessage {
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

    /// Convert HandleMessage into StoredMessage.
    pub fn into_stored(self, source: ActorId) -> StoredMessage {
        self.into_message(source).into()
    }

    /// Convert HandleMessage into Dispatch.
    pub fn into_dispatch(self, source: ActorId) -> Dispatch {
        Dispatch::new(DispatchKind::Handle, self.into_message(source))
    }

    /// Convert HandleMessage into StoredDispatch.
    pub fn into_stored_dispatch(self, source: ActorId) -> StoredDispatch {
        self.into_dispatch(source).into()
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
        &self.payload
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
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(any(feature = "mock", test), derive(Default))]
pub struct HandlePacket {
    /// Packet destination.
    destination: ActorId,
    /// Packet payload.
    payload: Payload,
    /// Packet optional gas limit.
    gas_limit: Option<GasLimit>,
    /// Packet value.
    value: Value,
}

impl HandlePacket {
    /// Create new packet without gas.
    pub fn new(destination: ActorId, payload: Payload, value: Value) -> Self {
        Self {
            destination,
            payload,
            gas_limit: None,
            value,
        }
    }

    /// Create new packet with gas.
    pub fn new_with_gas(
        destination: ActorId,
        payload: Payload,
        gas_limit: GasLimit,
        value: Value,
    ) -> Self {
        Self {
            destination,
            payload,
            gas_limit: Some(gas_limit),
            value,
        }
    }

    /// Create new packet with optional gas.
    pub fn maybe_with_gas(
        destination: ActorId,
        payload: Payload,
        gas_limit: Option<GasLimit>,
        value: Value,
    ) -> Self {
        match gas_limit {
            None => Self::new(destination, payload, value),
            Some(gas_limit) => Self::new_with_gas(destination, payload, gas_limit, value),
        }
    }

    /// Prepend payload.
    pub(super) fn try_prepend(&mut self, mut data: Payload) -> Result<(), Payload> {
        if data.try_extend_from_slice(self.payload_bytes()).is_err() {
            Err(data)
        } else {
            self.payload = data;
            Ok(())
        }
    }

    /// Packet destination.
    pub fn destination(&self) -> ActorId {
        self.destination
    }
}

impl Packet for HandlePacket {
    fn payload_bytes(&self) -> &[u8] {
        &self.payload
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
        DispatchKind::Handle
    }
}
