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
    message::{DispatchKind, GasLimit, Payload, StatusCode, StoredDispatch, StoredMessage, Value},
};
use alloc::string::ToString;
use core::{convert::TryInto, ops::Deref};
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

/// An entity that is used for interaction between actors.
/// Can transfer value and executes by programs in corresponding function: init, handle or handle_reply.
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
    /// Message details like reply message ID, status code, etc.
    details: Option<MessageDetails>,
}

impl From<Message> for StoredMessage {
    fn from(message: Message) -> StoredMessage {
        StoredMessage::new(
            message.id,
            message.source,
            message.destination,
            message.payload,
            message.value,
            message.details,
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
        details: Option<MessageDetails>,
    ) -> Self {
        Self {
            id,
            source,
            destination,
            payload,
            gas_limit,
            value,
            details,
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
        self.payload.get()
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
    pub fn reply(&self) -> Option<ReplyDetails> {
        self.details.and_then(|d| d.to_reply_details())
    }

    /// Status code of the message, if reply or signal.
    pub fn status_code(&self) -> Option<StatusCode> {
        self.details.map(|d| d.status_code())
    }

    #[allow(clippy::result_large_err)]
    /// Consumes self in order to create new `StoredMessage`, which payload
    /// contains string representation of initial bytes,
    /// decoded into given type.
    pub fn with_string_payload<D: Decode + ToString>(self) -> Result<Self, Self> {
        if let Ok(decoded) = D::decode(&mut self.payload.get()) {
            if let Ok(payload) = decoded.to_string().into_bytes().try_into() {
                Ok(Self { payload, ..self })
            } else {
                Err(self)
            }
        } else {
            Err(self)
        }
    }

    /// Returns bool defining if message is error reply.
    pub fn is_error_reply(&self) -> bool {
        self.details.map(|d| d.is_error_reply()).unwrap_or(false)
    }
}

/// Message details data.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Decode,
    Encode,
    TypeInfo,
    derive_more::From,
)]
pub enum MessageDetails {
    /// Reply details.
    Reply(ReplyDetails),
    /// Message details.
    Signal(SignalDetails),
}

impl MessageDetails {
    /// Returns bool defining if message is error reply.
    pub fn is_error_reply(&self) -> bool {
        self.is_reply_details() && self.status_code() != 0
    }

    /// Returns status code.
    pub fn status_code(&self) -> StatusCode {
        match self {
            MessageDetails::Reply(ReplyDetails { status_code, .. })
            | MessageDetails::Signal(SignalDetails { status_code, .. }) => *status_code,
        }
    }

    /// Check if kind is reply.
    pub fn is_reply_details(&self) -> bool {
        matches!(self, Self::Reply(_))
    }

    /// Returns reply details.
    pub fn to_reply_details(self) -> Option<ReplyDetails> {
        match self {
            MessageDetails::Reply(reply) => Some(reply),
            MessageDetails::Signal(_) => None,
        }
    }

    /// Check if kind is signal.
    pub fn is_signal_details(&self) -> bool {
        matches!(self, Self::Signal(_))
    }

    /// Reply signal details.
    pub fn to_signal_details(self) -> Option<SignalDetails> {
        match self {
            MessageDetails::Reply(_) => None,
            MessageDetails::Signal(signal) => Some(signal),
        }
    }
}

/// Reply details data.
///
/// Part of [`ReplyMessage`](crate::message::ReplyMessage) logic, containing data about on which message id
/// this replies and its status code.
#[derive(
    Clone, Copy, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo,
)]
pub struct ReplyDetails {
    /// Message id, this message replies on.
    reply_to: MessageId,
    /// Status code of the reply.
    status_code: StatusCode,
}

impl ReplyDetails {
    /// Constructor for details.
    pub fn new(reply_to: MessageId, status_code: StatusCode) -> Self {
        Self {
            reply_to,
            status_code,
        }
    }

    /// Message id getter.
    pub fn reply_to(&self) -> MessageId {
        self.reply_to
    }

    /// Status code getter.
    pub fn status_code(&self) -> StatusCode {
        self.status_code
    }

    /// Destructs self in parts of components.
    pub fn into_parts(self) -> (MessageId, StatusCode) {
        (self.reply_to, self.status_code)
    }

    /// Destructs self in `MessageId` replied to.
    pub fn into_reply_to(self) -> MessageId {
        self.reply_to
    }

    /// Destructs self in `StatusCode` replied with.
    pub fn into_status_code(self) -> StatusCode {
        self.status_code
    }
}

/// Signal details data.
#[derive(
    Clone, Copy, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo,
)]
pub struct SignalDetails {
    /// Message id, which issues signal.
    from: MessageId,
    /// Status code of the reply.
    status_code: StatusCode,
}

impl SignalDetails {
    /// Constructor for details.
    pub fn new(from: MessageId, status_code: StatusCode) -> Self {
        Self { from, status_code }
    }

    /// Message id getter.
    pub fn from(&self) -> MessageId {
        self.from
    }

    /// Status code getter.
    pub fn status_code(&self) -> StatusCode {
        self.status_code
    }

    /// Destructs self in parts of components.
    pub fn into_parts(self) -> (MessageId, StatusCode) {
        (self.from, self.status_code)
    }

    /// Destructs self in `MessageId` which issues signal.
    pub fn into_from(self) -> MessageId {
        self.from
    }

    /// Destructs self in `StatusCode` replied with.
    pub fn into_status_code(self) -> StatusCode {
        self.status_code
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
