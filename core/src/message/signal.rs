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
    message::{Dispatch, DispatchKind, Message, SignalDetails, StatusCode},
};
use gear_core_errors::{SimpleCodec, SimpleSignalError};
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

/// Message for signal entry point.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct SignalMessage {
    /// Message id.
    id: MessageId,
    /// Reply status code.
    status_code: StatusCode,
}

impl SignalMessage {
    /// Creates a new [`SignalMessage`].
    pub fn new(origin_msg_id: MessageId, err: SimpleSignalError) -> Self {
        let id = MessageId::generate_signal(origin_msg_id);
        let status_code = err.into_status_code();

        Self { id, status_code }
    }

    /// Convert [`SignalMessage`] into [`Message`].
    pub fn into_message(self, origin_msg_id: MessageId, destination: ProgramId) -> Message {
        Message::new(
            self.id,
            ProgramId::SYSTEM,
            destination,
            Default::default(),
            None,
            0,
            Some(SignalDetails::new(origin_msg_id, self.status_code).into()),
        )
    }

    /// Convert [`SignalMessage`] into [`Dispatch`].
    pub fn into_dispatch(self, origin_msg_id: MessageId, destination: ProgramId) -> Dispatch {
        Dispatch::new(
            DispatchKind::Signal,
            self.into_message(origin_msg_id, destination),
        )
    }

    /// Message id.
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Status code of the reply message.
    pub fn status_code(&self) -> StatusCode {
        self.status_code
    }
}
