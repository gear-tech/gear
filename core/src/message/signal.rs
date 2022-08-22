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
    message::{Dispatch, DispatchKind, ExitCode, Message, Payload, ReplyDetails},
};
use codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Message for Reply entry point.
/// [`ReplyMessage`] is unique because of storing [`MessageId`] from message on what it replies, and can be the only one per some message execution.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct SignalMessage {
    /// Message id.
    id: MessageId,
    /// Message payload.
    payload: Payload,
    /// Reply exit code.
    exit_code: ExitCode,
}

impl SignalMessage {
    /// Creates a new [`SignalMessage`].
    pub fn new(id: MessageId, payload: Payload, exit_code: ExitCode) -> Self {
        Self {
            id,
            payload,
            exit_code,
        }
    }

    /// Convert [`SignalMessage`] into [`Message`].
    pub fn into_message(self, destination: ProgramId) -> Message {
        Message::new(
            self.id,
            ProgramId::SYSTEM,
            destination,
            self.payload,
            None,
            0,
            Some(ReplyDetails::new(self.id, self.exit_code)),
        )
    }

    /// Convert [`SignalMessage`] into [`Dispatch`].
    pub fn into_dispatch(self, destination: ProgramId) -> Dispatch {
        Dispatch::new(DispatchKind::Signal, self.into_message(destination))
    }

    /// Message id.
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Message payload reference.
    pub fn payload(&self) -> &[u8] {
        self.payload.as_ref()
    }

    /// Exit code of the reply message.
    pub fn exit_code(&self) -> ExitCode {
        self.exit_code
    }
}
