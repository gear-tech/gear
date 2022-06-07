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

use alloc::vec::Vec;
use codec::{Decode, Encode};
use scale_info::TypeInfo;

mod common;
mod context;
mod handle;
mod incoming;
mod init;
mod reply;
mod stored;

pub use common::{Dispatch, Message};
pub use context::{ContextOutcome, ContextSettings, ContextStore, MessageContext};
pub use handle::{HandleMessage, HandlePacket};
pub use incoming::{IncomingDispatch, IncomingMessage};
pub use init::{InitMessage, InitPacket};
pub use reply::{ReplyMessage, ReplyPacket};
pub use stored::{StoredDispatch, StoredMessage};

/// Payload type for message.
pub type Payload = Vec<u8>;

/// Gas limit type for message.
pub type GasLimit = u64;

/// Value type for message.
pub type Value = u128;

/// Exit code type for message replies.
pub type ExitCode = i32;

/// Salt type for init message.
pub type Salt = Vec<u8>;

/// Entry point for dispatch processing.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub enum DispatchKind {
    /// Initialization.
    Init,
    /// Common handle.
    Handle,
    /// Handle reply.
    Reply,
}

impl DispatchKind {
    /// Convert DispatchKind into entry point function name.
    pub fn into_entry(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Handle => "handle",
            Self::Reply => "handle_reply",
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
}

/// Message packet.
///
/// Provides common behaviour for any message's packet: accessing to payload, gas limit and value.
pub trait Packet {
    /// Packet payload reference.
    fn payload(&self) -> &[u8];

    /// Packet optional gas limit.
    fn gas_limit(&self) -> Option<GasLimit>;

    /// Packet value.
    fn value(&self) -> Value;
}
