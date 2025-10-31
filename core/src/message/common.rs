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
        DispatchKind, GasLimit, StoredDelayedDispatch, StoredDispatch, StoredMessage, Value,
    },
};
use core::ops::Deref;
use gear_core_errors::{ReplyCode, SignalCode};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// An entity that is used for interaction between actors.
/// Can transfer value and executes by programs in corresponding function: init, handle or handle_reply.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode)]
pub struct Message {
    /// Message id.
    id: MessageId,
    /// Message source.
    source: ActorId,
    /// Message destination.
    destination: ActorId,
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
        source: ActorId,
        destination: ActorId,
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

    /// Into parts.
    pub fn into_parts(
        self,
    ) -> (
        MessageId,
        ActorId,
        ActorId,
        Payload,
        Option<GasLimit>,
        Value,
        Option<MessageDetails>,
    ) {
        (
            self.id,
            self.source,
            self.destination,
            self.payload,
            self.gas_limit,
            self.value,
            self.details,
        )
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

    /// Message optional gas limit.
    pub fn gas_limit(&self) -> Option<GasLimit> {
        self.gas_limit
    }

    /// Message value.
    pub fn value(&self) -> Value {
        self.value
    }

    /// Message reply.
    pub fn reply_details(&self) -> Option<ReplyDetails> {
        self.details.and_then(|d| d.to_reply_details())
    }

    /// Message signal.
    pub fn signal_details(&self) -> Option<SignalDetails> {
        self.details.and_then(|d| d.to_signal_details())
    }

    /// Returns bool defining if message is error reply.
    pub fn is_error_reply(&self) -> bool {
        self.details.map(|d| d.is_error_reply()).unwrap_or(false)
    }

    /// Returns bool defining if message is reply.
    pub fn is_reply(&self) -> bool {
        self.reply_details().is_some()
    }
}

/// Message details data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Decode, Encode, TypeInfo, derive_more::From)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum MessageDetails {
    /// Reply details.
    Reply(ReplyDetails),
    /// Message details.
    Signal(SignalDetails),
}

impl MessageDetails {
    /// Returns bool defining if message is error reply.
    pub fn is_error_reply(&self) -> bool {
        self.to_reply_details()
            .map(|d| d.code.is_error())
            .unwrap_or(false)
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Decode, Encode, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ReplyDetails {
    /// Message id, this message replies on.
    to: MessageId,
    /// Reply code.
    code: ReplyCode,
}

impl ReplyDetails {
    /// Constructor for details.
    pub fn new(to: MessageId, code: ReplyCode) -> Self {
        Self { to, code }
    }

    /// Returns message id replied to.
    pub fn to_message_id(&self) -> MessageId {
        self.to
    }

    /// Returns reply code of reply details.
    pub fn to_reply_code(&self) -> ReplyCode {
        self.code
    }

    /// Destructs details into parts.
    pub fn into_parts(self) -> (MessageId, ReplyCode) {
        (self.to, self.code)
    }
}

/// Signal details data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Decode, Encode, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct SignalDetails {
    /// Message id, which issues signal.
    to: MessageId,
    /// Signal code.
    code: SignalCode,
}

impl SignalDetails {
    /// Constructor for details.
    pub fn new(to: MessageId, code: SignalCode) -> Self {
        Self { to, code }
    }

    /// Returns message id signal sent from.
    pub fn to_message_id(&self) -> MessageId {
        self.to
    }

    /// Returns signal code of signal details.
    pub fn to_signal_code(&self) -> SignalCode {
        self.code
    }

    /// Destructs details into parts.
    pub fn into_parts(self) -> (MessageId, SignalCode) {
        (self.to, self.code)
    }
}

/// Message with entry point.
#[derive(Clone, Debug, PartialEq, Eq, Decode, Encode)]
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

impl From<Dispatch> for StoredDelayedDispatch {
    fn from(dispatch: Dispatch) -> StoredDelayedDispatch {
        StoredDelayedDispatch::new(dispatch.kind, dispatch.message.into())
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

    /// Convert Dispatch into gasless StoredDelayedDispatch.
    pub fn into_stored_delayed(self) -> StoredDelayedDispatch {
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
