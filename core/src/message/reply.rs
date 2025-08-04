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

use super::common::ReplyDetails;
use crate::{
    buffer::Payload,
    ids::{ActorId, MessageId, prelude::*},
    message::{
        Dispatch, DispatchKind, GasLimit, Message, Packet, StoredDispatch, StoredMessage, Value,
    },
};
use gear_core_errors::{ErrorReplyReason, ReplyCode, SuccessReplyReason};

/// Message for Reply entry point.
/// [`ReplyMessage`] is unique because of storing [`MessageId`] from message on what it replies, and can be the only one per some message execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyMessage {
    /// Message id.
    id: MessageId,
    /// Message payload.
    payload: Payload,
    /// Message optional gas limit.
    gas_limit: Option<GasLimit>,
    /// Message value.
    value: Value,
    /// Reply status code.
    code: ReplyCode,
}

impl ReplyMessage {
    /// Create ReplyMessage from ReplyPacket.
    pub fn from_packet(id: MessageId, packet: ReplyPacket) -> Self {
        Self {
            id,
            payload: packet.payload,
            gas_limit: packet.gas_limit,
            value: packet.value,
            code: packet.code,
        }
    }

    /// Create new system generated ReplyMessage.
    pub fn system(
        origin_msg_id: MessageId,
        payload: Payload,
        value: Value,
        err: impl Into<ErrorReplyReason>,
    ) -> Self {
        let id = MessageId::generate_reply(origin_msg_id);
        let packet = ReplyPacket::system(payload, value, err);

        Self::from_packet(id, packet)
    }

    /// Create new auto-generated ReplyMessage.
    pub fn auto(origin_msg_id: MessageId) -> Self {
        let id = MessageId::generate_reply(origin_msg_id);
        let packet = ReplyPacket::auto();

        Self::from_packet(id, packet)
    }

    /// Convert ReplyMessage into Message.
    pub fn into_message(
        self,
        program_id: ActorId,
        destination: ActorId,
        origin_msg_id: MessageId,
    ) -> Message {
        Message::new(
            self.id,
            program_id,
            destination,
            self.payload,
            self.gas_limit,
            self.value,
            Some(ReplyDetails::new(origin_msg_id, self.code).into()),
        )
    }

    /// Convert ReplyMessage into StoredMessage.
    pub fn into_stored(
        self,
        program_id: ActorId,
        destination: ActorId,
        origin_msg_id: MessageId,
    ) -> StoredMessage {
        self.into_message(program_id, destination, origin_msg_id)
            .into()
    }

    /// Convert ReplyMessage into Dispatch.
    pub fn into_dispatch(
        self,
        source: ActorId,
        destination: ActorId,
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
        source: ActorId,
        destination: ActorId,
        origin_msg_id: MessageId,
    ) -> StoredDispatch {
        self.into_dispatch(source, destination, origin_msg_id)
            .into()
    }

    /// Message id.
    pub fn id(&self) -> MessageId {
        self.id
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

    /// Reply code of the message.
    pub fn code(&self) -> ReplyCode {
        self.code
    }
}

/// Reply message packet.
///
/// This structure is preparation for future reply message sending. Has no message id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyPacket {
    /// Message payload.
    payload: Payload,
    /// Message optional gas limit.
    gas_limit: Option<GasLimit>,
    /// Message value.
    value: Value,
    /// Reply status code.
    code: ReplyCode,
}

#[cfg(test)]
impl Default for ReplyPacket {
    fn default() -> Self {
        Self::auto()
    }
}

impl ReplyPacket {
    /// Create new manual ReplyPacket without gas.
    pub fn new(payload: Payload, value: Value) -> Self {
        Self {
            payload,
            gas_limit: None,
            value,
            code: ReplyCode::Success(SuccessReplyReason::Manual),
        }
    }

    /// Create new manual ReplyPacket with gas.
    pub fn new_with_gas(payload: Payload, gas_limit: GasLimit, value: Value) -> Self {
        Self {
            payload,
            gas_limit: Some(gas_limit),
            value,
            code: ReplyCode::Success(SuccessReplyReason::Manual),
        }
    }

    /// Create new manual ReplyPacket with optional gas.
    pub fn maybe_with_gas(payload: Payload, gas_limit: Option<GasLimit>, value: Value) -> Self {
        match gas_limit {
            None => Self::new(payload, value),
            Some(gas_limit) => Self::new_with_gas(payload, gas_limit, value),
        }
    }

    // TODO: consider using here `impl CoreError` and/or provide `AsStatusCode`
    // trait or append such functionality to `CoreError` (issue #1083).
    /// Create new system generated ReplyPacket.
    pub fn system(payload: Payload, value: Value, err: impl Into<ErrorReplyReason>) -> Self {
        Self {
            payload,
            gas_limit: None,
            value,
            code: ReplyCode::error(err),
        }
    }

    /// Auto-generated reply after success execution.
    pub fn auto() -> Self {
        Self {
            payload: Default::default(),
            gas_limit: Some(0),
            value: 0,
            code: ReplyCode::Success(SuccessReplyReason::Auto),
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

    /// Packet status code.
    pub fn code(&self) -> ReplyCode {
        self.code
    }
}

impl Packet for ReplyPacket {
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
        DispatchKind::Reply
    }
}
