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
use core::convert::TryFrom;
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

use super::StoredMessage;

/// Message sent to user's mailbox.
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
    /// Message details: reply message ID and reply code if exists.x
    details: Option<ReplyDetails>,
}

impl UserMessage {
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

    /// Message value.
    pub fn value(&self) -> Value {
        self.value
    }

    /// Message reply details.
    pub fn details(&self) -> Option<ReplyDetails> {
        self.details
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct UserMessageConvertError;

impl TryFrom<StoredMessage> for UserMessage {
    type Error = UserMessageConvertError;

    fn try_from(stored: StoredMessage) -> Result<Self, Self::Error> {
        let some_details = stored.details.is_some();
        let details = stored.details.and_then(|d| d.to_reply_details());

        if details.is_none() && some_details {
            return Err(UserMessageConvertError);
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
