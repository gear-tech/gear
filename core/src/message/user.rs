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

use super::{MessageDetails, StoredMessage};
use crate::{
    buffer::Payload,
    ids::{ActorId, MessageId},
    message::{ReplyDetails, Value},
};
use core::convert::TryFrom;
use gear_core_errors::ReplyCode;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Message sent to user and deposited as event.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Decode, Encode, TypeInfo)]
pub struct UserMessage {
    /// Message id.
    id: MessageId,
    /// Message source.
    source: ActorId,
    /// Message destination.
    destination: ActorId,
    /// Message payload.
    payload: Payload,
    /// Message value.
    #[codec(compact)]
    value: Value,
    /// Message details: reply message ID and reply code if exists.
    details: Option<ReplyDetails>,
}

impl UserMessage {
    /// Create new UserMessage.
    pub fn new(
        id: MessageId,
        source: ActorId,
        destination: ActorId,
        payload: Payload,
        value: Value,
        details: Option<ReplyDetails>,
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

    /// Message reply details.
    pub fn details(&self) -> Option<ReplyDetails> {
        self.details
    }

    /// Returns `ReplyCode` of message if reply.
    pub fn reply_code(&self) -> Option<ReplyCode> {
        self.details.map(|d| d.to_reply_code())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FromStoredMessageError;

impl TryFrom<StoredMessage> for UserMessage {
    type Error = FromStoredMessageError;

    fn try_from(stored: StoredMessage) -> Result<Self, Self::Error> {
        let some_details = stored.details.is_some();
        let details = stored.details.and_then(|d| d.to_reply_details());

        if details.is_none() && some_details {
            return Err(FromStoredMessageError);
        }

        Ok(Self {
            id: stored.id,
            source: stored.source,
            destination: stored.destination,
            payload: stored.payload,
            value: stored.value,
            details,
        })
    }
}

impl From<UserMessage> for StoredMessage {
    fn from(user: UserMessage) -> Self {
        let details = user.details.map(MessageDetails::Reply);

        StoredMessage {
            id: user.id,
            source: user.source,
            destination: user.destination,
            payload: user.payload,
            value: user.value,
            details,
        }
    }
}

/// Message sent to user and added to mailbox.
///
/// May be represented only with `DispatchKind::Handle`,
/// so does not contain message details.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Decode, Encode, TypeInfo)]
pub struct UserStoredMessage {
    /// Message id.
    id: MessageId,
    /// Message source.
    source: ActorId,
    /// Message destination.
    destination: ActorId,
    /// Message payload.
    payload: Payload,
    /// Message value.
    #[codec(compact)]
    value: Value,
}

impl UserStoredMessage {
    /// Create new UserStoredMessage.
    pub fn new(
        id: MessageId,
        source: ActorId,
        destination: ActorId,
        payload: Payload,
        value: Value,
    ) -> Self {
        Self {
            id,
            source,
            destination,
            payload,
            value,
        }
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
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserStoredMessageConvertError;

impl TryFrom<StoredMessage> for UserStoredMessage {
    type Error = UserStoredMessageConvertError;

    fn try_from(stored: StoredMessage) -> Result<Self, Self::Error> {
        if stored.details().is_some() {
            return Err(UserStoredMessageConvertError);
        }

        Ok(Self {
            id: stored.id,
            source: stored.source,
            destination: stored.destination,
            payload: stored.payload,
            value: stored.value,
        })
    }
}

impl TryFrom<UserMessage> for UserStoredMessage {
    type Error = UserStoredMessageConvertError;

    fn try_from(user: UserMessage) -> Result<Self, Self::Error> {
        if user.details().is_some() {
            return Err(UserStoredMessageConvertError);
        }

        Ok(Self {
            id: user.id,
            source: user.source,
            destination: user.destination,
            payload: user.payload,
            value: user.value,
        })
    }
}

impl From<UserStoredMessage> for StoredMessage {
    fn from(user_stored: UserStoredMessage) -> Self {
        StoredMessage {
            id: user_stored.id,
            source: user_stored.source,
            destination: user_stored.destination,
            payload: user_stored.payload,
            value: user_stored.value,
            details: None,
        }
    }
}
