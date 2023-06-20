// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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
    message::{Payload, ReplyDetails, Value},
};
use alloc::string::ToString;
use core::convert::TryFrom;
use gear_core_errors::ReplyCode;
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

use super::{MessageDetails, StoredMessage};

/// Message sent to user and deposited as event.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct UserMessage {
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
    /// Message details: reply message ID and reply code if exists.
    details: Option<ReplyDetails>,
}

impl UserMessage {
    /// Create new UserMessage.
    pub fn new(
        id: MessageId,
        source: ProgramId,
        destination: ProgramId,
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
    pub fn source(&self) -> ProgramId {
        self.source
    }

    /// Message destination.
    pub fn destination(&self) -> ProgramId {
        self.destination
    }

    /// Message payload bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.inner()
    }

    /// Message value.
    pub fn value(&self) -> Value {
        self.value
    }

    /// Message reply details.
    pub fn details(&self) -> Option<ReplyDetails> {
        self.details
    }

    #[allow(clippy::result_large_err)]
    /// Consumes self in order to create new `StoredMessage`, which payload
    /// contains string representation of initial bytes,
    /// decoded into given type.
    // TODO: issue #2849.
    pub fn with_string_payload<D: Decode + ToString>(self) -> Result<Self, Self> {
        if let Ok(decoded) = D::decode(&mut self.payload.inner()) {
            if let Ok(payload) = decoded.to_string().into_bytes().try_into() {
                Ok(Self { payload, ..self })
            } else {
                Err(self)
            }
        } else {
            Err(self)
        }
    }

    /// Returns `ReplyCode` of message if reply.
    pub fn reply_code(&self) -> Option<ReplyCode> {
        self.details.map(|d| d.to_reply_code())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
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
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct UserStoredMessage {
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
}

impl UserStoredMessage {
    /// Create new UserStoredMessage.
    pub fn new(
        id: MessageId,
        source: ProgramId,
        destination: ProgramId,
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
    pub fn source(&self) -> ProgramId {
        self.source
    }

    /// Message destination.
    pub fn destination(&self) -> ProgramId {
        self.destination
    }

    /// Message payload bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.inner()
    }

    /// Message value.
    pub fn value(&self) -> Value {
        self.value
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
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
