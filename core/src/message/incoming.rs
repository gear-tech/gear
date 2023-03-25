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
        common::MessageDetails, ContextStore, DispatchKind, GasLimit, Payload, StoredDispatch,
        StoredMessage, Value,
    },
};
use core::ops::Deref;
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

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
    /// Message details like reply message ID, status code, etc.
    details: Option<MessageDetails>,
}

impl IncomingMessage {
    /// Create new IncomingMessage.
    pub fn new(
        id: MessageId,
        source: ProgramId,
        payload: Payload,
        gas_limit: GasLimit,
        value: Value,
        details: Option<MessageDetails>,
    ) -> Self {
        Self {
            id,
            source,
            payload,
            gas_limit,
            value,
            details,
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
            self.details,
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
        self.payload.get()
    }

    /// Message gas limit.
    pub fn gas_limit(&self) -> GasLimit {
        self.gas_limit
    }

    /// Message value.
    pub fn value(&self) -> Value {
        self.value
    }

    /// Message details.
    pub fn details(&self) -> Option<MessageDetails> {
        self.details
    }

    /// Returns bool defining if message is error reply.
    pub fn is_error_reply(&self) -> bool {
        self.details.map(|d| d.is_error_reply()).unwrap_or(false)
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

    /// Decompose IncomingDispatch for it's components: DispatchKind, IncomingMessage and `Option<ContextStore>`.
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

    /// Previous execution context mutable reference, if exists.
    pub fn context_mut(&mut self) -> &mut Option<ContextStore> {
        &mut self.context
    }
}

impl Deref for IncomingDispatch {
    type Target = IncomingMessage;

    fn deref(&self) -> &Self::Target {
        self.message()
    }
}
