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
        ContextStore, DispatchKind, GasLimit, IncomingDispatch, IncomingMessage, ReplyDetails,
        Value, common::MessageDetails,
    },
};
use core::ops::Deref;
use gear_core_errors::ReplyCode;
use parity_scale_codec::{Decode, Encode};
use scale_decode::DecodeAsType;
use scale_encode::EncodeAsType;
use scale_info::TypeInfo;

/// Stored message.
///
/// Gasless Message for storing.
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, Decode, DecodeAsType, Encode, EncodeAsType, TypeInfo,
)]
pub struct StoredMessage {
    /// Message id.
    pub(super) id: MessageId,
    /// Message source.
    pub(super) source: ActorId,
    /// Message destination.
    pub(super) destination: ActorId,
    /// Message payload.
    pub(super) payload: Payload,
    /// Message value.
    #[codec(compact)]
    pub(super) value: Value,
    /// Message details like reply message ID, status code, etc.
    pub(super) details: Option<MessageDetails>,
}

impl StoredMessage {
    /// Create new StoredMessage.
    pub fn new(
        id: MessageId,
        source: ActorId,
        destination: ActorId,
        payload: Payload,
        value: Value,
        details: Option<MessageDetails>,
    ) -> Self {
        Self {
            id,
            source,
            destination,
            payload,
            value,
            details,
        }
    }

    /// Into parts.
    pub fn into_parts(
        self,
    ) -> (
        MessageId,
        ActorId,
        ActorId,
        Payload,
        Value,
        Option<MessageDetails>,
    ) {
        (
            self.id,
            self.source,
            self.destination,
            self.payload,
            self.value,
            self.details,
        )
    }

    /// Convert StoredMessage into IncomingMessage for program processing.
    pub fn into_incoming(self, gas_limit: GasLimit) -> IncomingMessage {
        IncomingMessage::new(
            self.id,
            self.source,
            self.payload,
            gas_limit,
            self.value,
            self.details,
        )
    }

    /// Message id.
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Message source.
    pub fn source(&self) -> ActorId {
        self.source
    }

    /// Message destination.
    pub fn destination(&self) -> ActorId {
        self.destination
    }

    /// Message payload bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        &self.payload
    }

    /// Message value.
    pub fn value(&self) -> Value {
        self.value
    }

    /// Message details.
    pub fn details(&self) -> Option<MessageDetails> {
        self.details
    }

    /// Message reply.
    pub fn reply_details(&self) -> Option<ReplyDetails> {
        self.details.and_then(|d| d.to_reply_details())
    }

    /// Returns bool defining if message is error reply.
    pub fn is_error_reply(&self) -> bool {
        self.details.map(|d| d.is_error_reply()).unwrap_or(false)
    }

    /// Returns bool defining if message is reply.
    pub fn is_reply(&self) -> bool {
        self.details.map(|d| d.is_reply_details()).unwrap_or(false)
    }

    /// Returns `ReplyCode` of message if reply.
    pub fn reply_code(&self) -> Option<ReplyCode> {
        self.details
            .and_then(|d| d.to_reply_details().map(|d| d.to_reply_code()))
    }
}

/// Stored message with entry point and previous execution context, if exists.
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, Decode, DecodeAsType, Encode, EncodeAsType, TypeInfo,
)]
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

    /// Decompose StoredDispatch for it's components: DispatchKind, StoredMessage and `Option<ContextStore>`.
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

impl From<StoredDelayedDispatch> for StoredDispatch {
    fn from(dispatch: StoredDelayedDispatch) -> Self {
        StoredDispatch::new(dispatch.kind, dispatch.message, None)
    }
}

/// Stored message with entry point.
///
/// We could use just [`StoredDispatch`]
/// but delayed messages always don't have [`ContextStore`]
/// so we designate this fact via new type.
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, Decode, DecodeAsType, Encode, EncodeAsType, TypeInfo,
)]
pub struct StoredDelayedDispatch {
    /// Entry point.
    kind: DispatchKind,
    /// Stored message.
    message: StoredMessage,
}

impl From<StoredDelayedDispatch> for (DispatchKind, StoredMessage) {
    fn from(dispatch: StoredDelayedDispatch) -> (DispatchKind, StoredMessage) {
        (dispatch.kind, dispatch.message)
    }
}

impl StoredDelayedDispatch {
    /// Create new StoredDelayedDispatch.
    pub fn new(kind: DispatchKind, message: StoredMessage) -> Self {
        Self { kind, message }
    }

    /// Decompose StoredDelayedDispatch for it's components: DispatchKind, StoredMessage.
    pub fn into_parts(self) -> (DispatchKind, StoredMessage) {
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
}

impl Deref for StoredDelayedDispatch {
    type Target = StoredMessage;

    fn deref(&self) -> &Self::Target {
        self.message()
    }
}
