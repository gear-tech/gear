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

use crate::ids::{MessageId, ProgramId};
use crate::message::{
    ContextStore, DispatchKind, ExitCode, GasLimit, IncomingDispatch, IncomingMessage, Payload,
    Value,
};
use codec::{Decode, Encode};
use core::ops::Deref;
use scale_info::TypeInfo;

/// Stored message.
///
/// Gasless Message for storing.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct StoredMessage {
    /// Message id.
    id: MessageId,
    /// Message source.
    source: ProgramId,
    /// Message destionation.
    destination: ProgramId,
    /// Message payload.
    payload: Payload,
    /// Message value.
    value: Value,
    /// Message id replied on with exit code.
    reply: Option<(MessageId, ExitCode)>,
}

impl StoredMessage {
    /// Create new StoredMessage.
    pub fn new(
        id: MessageId,
        source: ProgramId,
        destination: ProgramId,
        payload: Payload,
        value: Value,
        reply: Option<(MessageId, ExitCode)>,
    ) -> Self {
        Self {
            id,
            source,
            destination,
            payload,
            value,
            reply,
        }
    }

    /// Convert StoredMessage into IncomingMessage for program processing.
    pub fn into_incoming(self, gas_limit: GasLimit) -> IncomingMessage {
        IncomingMessage::new(
            self.id,
            self.source,
            self.payload,
            gas_limit,
            self.value,
            self.reply,
        )
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

/// Stored message with entry point and previous execution context, if exists.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct StoredDispatch {
    /// Entry point.
    kind: DispatchKind,
    /// Stored message.
    message: StoredMessage,
    /// Previous execution context.
    context: Option<ContextStore>,
}

impl StoredDispatch {
    /// Create new StoredDispatch.
    pub fn new(kind: DispatchKind, message: StoredMessage, context: Option<ContextStore>) -> Self {
        Self {
            kind,
            message,
            context,
        }
    }

    /// Convert StoredDispatch into IncomingDispatch for program processing.
    pub fn into_incoming(self, gas_limit: GasLimit) -> IncomingDispatch {
        IncomingDispatch::new(
            self.kind,
            self.message.into_incoming(gas_limit),
            self.context,
        )
    }

    /// Entry point for the message.
    pub fn kind(&self) -> DispatchKind {
        self.kind
    }

    /// Dispatch message reference.
    pub fn message(&self) -> &StoredMessage {
        &self.message
    }

    /// Previous execution context reference, if exists.
    pub fn context(&self) -> &Option<ContextStore> {
        &self.context
    }
}

impl Deref for StoredDispatch {
    type Target = StoredMessage;

    fn deref(&self) -> &Self::Target {
        self.message()
    }
}
