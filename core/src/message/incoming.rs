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
        ContextStore, DispatchKind, ExitCode, GasLimit, Payload, StoredDispatch, StoredMessage,
        Value,
    },
};
use codec::{Decode, Encode};
use core::ops::Deref;
use scale_info::TypeInfo;

/// Incoming message.
///
/// Used for program execution.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct IncomingMessage {
    /// Message id.
    id: MessageId,
    /// Message source.
    source: ProgramId,
    /// Message payload.
    payload: Payload,
    /// Message gas limit. Required here.
    gas_limit: GasLimit,
    /// Message value.
    value: Value,
    /// Message id replied on with exit code.
    reply: Option<ReplyDetails>,
}

impl IncomingMessage {
    /// Create new IncomingMessage.
    pub fn new(
        id: MessageId,
        source: ProgramId,
        payload: Payload,
        gas_limit: GasLimit,
        value: Value,
        reply: Option<ReplyDetails>,
    ) -> Self {
        Self {
            id,
            source,
            payload,
            gas_limit,
            value,
            reply,
        }
    }
    /// Convert IncomingMessage into gasless StoredMessage.
    pub fn into_stored(self, destination: ProgramId) -> StoredMessage {
        StoredMessage::new(
            self.id,
            self.source,
            destination,
            self.payload,
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

    /// Message payload reference.
    pub fn payload(&self) -> &[u8] {
        self.payload.as_ref()
    }

    /// Message gas limit.
    pub fn gas_limit(&self) -> GasLimit {
        self.gas_limit
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

    /// Returns bool defining if message is error reply.
    pub fn is_error_reply(&self) -> bool {
        !matches!(self.exit_code(), Some(0) | None)
    }
}

/// Incoming message with entry point and previous execution context, if exists.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct IncomingDispatch {
    /// Entry point.
    kind: DispatchKind,
    /// Incoming message.
    message: IncomingMessage,
    /// Previous execution context, if exists.
    context: Option<ContextStore>,
}

impl From<IncomingDispatch> for (DispatchKind, IncomingMessage, Option<ContextStore>) {
    fn from(dispatch: IncomingDispatch) -> (DispatchKind, IncomingMessage, Option<ContextStore>) {
        (dispatch.kind, dispatch.message, dispatch.context)
    }
}

impl IncomingDispatch {
    /// Create new IncomingDispatch.
    pub fn new(
        kind: DispatchKind,
        message: IncomingMessage,
        context: Option<ContextStore>,
    ) -> Self {
        Self {
            kind,
            message,
            context,
        }
    }

    /// Convert IncomingDispatch into gasless StoredDispatch with updated (or recently set) context.
    pub fn into_stored(self, destination: ProgramId, context: ContextStore) -> StoredDispatch {
        StoredDispatch::new(
            self.kind,
            self.message.into_stored(destination),
            Some(context),
        )
    }

    /// Decompose IncomingDispatch for it's components: DispatchKind, IncomingMessage and Option<ContextStore>.
    pub fn into_parts(self) -> (DispatchKind, IncomingMessage, Option<ContextStore>) {
        self.into()
    }

    /// Entry point for the message.
    pub fn kind(&self) -> DispatchKind {
        self.kind
    }

    /// Dispatch message reference.
    pub fn message(&self) -> &IncomingMessage {
        &self.message
    }

    /// Previous execution context reference, if exists.
    pub fn context(&self) -> &Option<ContextStore> {
        &self.context
    }
}

impl Deref for IncomingDispatch {
    type Target = IncomingMessage;

    fn deref(&self) -> &Self::Target {
        self.message()
    }
}
