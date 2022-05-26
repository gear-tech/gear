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
    message::{DispatchKind, ExitCode, GasLimit, Payload, StoredDispatch, StoredMessage, Value},
};
use codec::{Decode, Encode};
use core::ops::Deref;
use scale_info::TypeInfo;

/// A transaction that one actor sends to another.
/// Can transfer value and call one of the entry points in destination if it is a program.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct Message {
    /// Message id.
    id: MessageId,
    /// Message source.
    source: ProgramId,
    /// Message destination.
    destination: ProgramId,
    /// Message payload.
    payload: Payload,
    /// Message optional gas limit.
    gas_limit: Option<GasLimit>,
    /// Message value.
    value: Value,
    /// Message id replied on with exit code.
    reply: Option<(MessageId, ExitCode)>,
}

impl From<Message> for StoredMessage {
    fn from(message: Message) -> StoredMessage {
        StoredMessage::new(
            message.id,
            message.source,
            message.destination,
            message.payload,
            message.value,
            message.reply,
        )
    }
}

impl Message {
    /// Create new message.
    pub fn new(
        id: MessageId,
        source: ProgramId,
        destination: ProgramId,
        payload: Payload,
        gas_limit: Option<GasLimit>,
        value: Value,
        reply: Option<(MessageId, ExitCode)>,
    ) -> Self {
        Self {
            id,
            source,
            destination,
            payload,
            gas_limit,
            value,
            reply,
        }
    }

    /// Convert Message into gasless StoredMessage.
    pub fn into_stored(self) -> StoredMessage {
        self.into()
    }

    /// Message id.
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Message source.
    pub fn source(&self) -> ProgramId {
        self.source
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

    /// Message reply.
    pub fn reply(&self) -> Option<(MessageId, ExitCode)> {
        self.reply
    }

    /// Check if this message is reply.
    pub fn is_reply(&self) -> bool {
        self.reply.is_some()
    }

    /// Message id what this message replies to, if reply.
    pub fn reply_to(&self) -> Option<MessageId> {
        self.reply.map(|(id, _)| id)
    }

    /// Exit code of the message, if reply.
    pub fn exit_code(&self) -> Option<ExitCode> {
        self.reply.map(|(_, exit_code)| exit_code)
    }
}

/// Message with entry point.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct Dispatch {
    /// Entry point for the message.
    kind: DispatchKind,
    /// Message.
    message: Message,
}

impl From<Dispatch> for StoredDispatch {
    fn from(dispatch: Dispatch) -> StoredDispatch {
        StoredDispatch::new(dispatch.kind, dispatch.message.into(), None)
    }
}

impl From<Dispatch> for (DispatchKind, Message) {
    fn from(dispatch: Dispatch) -> (DispatchKind, Message) {
        (dispatch.kind, dispatch.message)
    }
}

impl Dispatch {
    /// Create new Dispatch.
    pub fn new(kind: DispatchKind, message: Message) -> Self {
        Self { kind, message }
    }

    /// Convert Dispatch into gasless StoredDispatch with empty previous context.
    pub fn into_stored(self) -> StoredDispatch {
        self.into()
    }

    /// Decompose Dispatch for it's components: DispatchKind and Message.
    pub fn into_parts(self) -> (DispatchKind, Message) {
        self.into()
    }

    /// Entry point for the message.
    pub fn kind(&self) -> DispatchKind {
        self.kind
    }

    /// Dispatch message reference.
    pub fn message(&self) -> &Message {
        &self.message
    }
}

impl Deref for Dispatch {
    type Target = Message;

    fn deref(&self) -> &Self::Target {
        self.message()
    }
}
