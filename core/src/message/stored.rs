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

use super::common::ReplyDetails;
use crate::{
    ids::{MessageId, ProgramId},
    message::{
        ContextStore, DispatchKind, ExitCode, GasLimit, IncomingDispatch, IncomingMessage, Payload,
        Value,
    },
};
use alloc::string::ToString;
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
    /// Message destination.
    destination: ProgramId,
    /// Message payload.
    payload: Payload,
    /// Message value.
    #[codec(compact)]
    value: Value,
    /// Message id replied on with exit code.
    reply: Option<ReplyDetails>,
}

impl StoredMessage {
    /// Create new StoredMessage.
    pub fn new(
        id: MessageId,
        source: ProgramId,
        destination: ProgramId,
        payload: Payload,
        value: Value,
        reply: Option<ReplyDetails>,
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
    pub fn reply(&self) -> Option<ReplyDetails> {
        self.reply
    }

    /// Check if this message is reply.
    pub fn is_reply(&self) -> bool {
        self.reply.is_some()
    }

    /// Message id what this message replies to, if reply.
    pub fn reply_to(&self) -> Option<MessageId> {
        self.reply.map(|v| v.reply_to())
    }

    /// Exit code of the message, if reply.
    pub fn exit_code(&self) -> Option<ExitCode> {
        self.reply.map(|v| v.exit_code())
    }

    #[allow(clippy::result_large_err)]
    /// Consumes self in order to create new `StoredMessage`, which payload
    /// contains string representation of initial bytes,
    /// decoded into given type.
    pub fn with_string_payload<D: Decode + ToString>(self) -> Result<Self, Self> {
        D::decode(&mut self.payload.as_ref())
            .map(|payload| {
                let payload = payload.to_string().into_bytes();
                Self { payload, ..self }
            })
            .map_err(|_| self)
    }

    /// Returns bool defining if message is error reply.
    pub fn is_error_reply(&self) -> bool {
        !matches!(self.exit_code(), Some(0) | None)
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

impl From<StoredDispatch> for (DispatchKind, StoredMessage, Option<ContextStore>) {
    fn from(dispatch: StoredDispatch) -> (DispatchKind, StoredMessage, Option<ContextStore>) {
        (dispatch.kind, dispatch.message, dispatch.context)
    }
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

    /// Decompose StoredDispatch for it's components: DispatchKind, StoredMessage and Option<ContextStore>.
    pub fn into_parts(self) -> (DispatchKind, StoredMessage, Option<ContextStore>) {
        self.into()
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
