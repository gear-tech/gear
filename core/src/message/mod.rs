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

//! Message processing module.

use codec::{Decode, Encode};
use scale_info::TypeInfo;

mod common;
mod context;
mod handle;
mod incoming;
mod init;
mod reply;
mod signal;
mod stored;

pub use common::{Dispatch, Message, MessageDetails, ReplyDetails, SignalDetails};
pub use context::{ContextOutcome, ContextSettings, ContextStore, MessageContext};
pub use handle::{HandleMessage, HandlePacket};
pub use incoming::{IncomingDispatch, IncomingMessage};
pub use init::{InitMessage, InitPacket};
pub use reply::{ReplyMessage, ReplyPacket};
pub use signal::SignalMessage;
pub use stored::{StoredDispatch, StoredMessage};

use core::fmt::Display;

use super::buffer::LimitedVec;

/// Max payload size which one message can have (8 MiB).
const MAX_PAYLOAD_SIZE: usize = 8 * 1024 * 1024;

/// Payload size exceed error
#[derive(
    Clone, Copy, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo,
)]
pub struct PayloadSizeError;

impl From<PayloadSizeError> for &str {
    fn from(_: PayloadSizeError) -> Self {
        "Payload size limit exceeded"
    }
}

impl Display for PayloadSizeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str((*self).into())
    }
}

/// Payload type for message.
pub type Payload = LimitedVec<u8, PayloadSizeError, MAX_PAYLOAD_SIZE>;

/// Gas limit type for message.
pub type GasLimit = u64;

/// Value type for message.
pub type Value = u128;

/// Status code type for message replies.
pub type StatusCode = i32;

/// Salt type for init message.
pub type Salt = LimitedVec<u8, PayloadSizeError, MAX_PAYLOAD_SIZE>;

/// Composite wait type for messages waiting.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, PartialOrd, Ord, TypeInfo)]
pub enum MessageWaitedType {
    /// Program called `gr_wait` while executing message.
    Wait,
    /// Program called `gr_wait_for` while executing message.
    WaitFor,
    /// Program called `gr_wait_up_to` with insufficient gas for full
    /// duration while executing message.
    WaitUpTo,
    /// Program called `gr_wait_up_to` with enough gas for full duration
    /// storing while executing message.
    WaitUpToFull,
}

/// Entry point for dispatch processing.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub enum DispatchKind {
    /// Initialization.
    Init,
    /// Common handle.
    Handle,
    /// Handle reply.
    Reply,
    /// System signal.
    Signal,
}

impl DispatchKind {
    /// Convert DispatchKind into entry point function name.
    pub fn into_entry(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Handle => "handle",
            Self::Reply => "handle_reply",
            Self::Signal => "handle_signal",
        }
    }

    /// Check if kind is init.
    pub fn is_init(&self) -> bool {
        matches!(self, Self::Init)
    }

    /// Check if kind is handle.
    pub fn is_handle(&self) -> bool {
        matches!(self, Self::Handle)
    }

    /// Check if kind is reply.
    pub fn is_reply(&self) -> bool {
        matches!(self, Self::Reply)
    }

    /// Check if kind is signal.
    pub fn is_signal(&self) -> bool {
        matches!(self, Self::Signal)
    }
}

/// Message packet.
///
/// Provides common behavior for any message's packet: accessing to payload, gas limit and value.
pub trait Packet {
    /// Packet payload reference.
    fn payload(&self) -> &[u8];

    /// Packet optional gas limit.
    fn gas_limit(&self) -> Option<GasLimit>;

    /// Packet value.
    fn value(&self) -> Value;
}
