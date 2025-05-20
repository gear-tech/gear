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

//! Message processing module.

mod common;
mod context;
mod handle;
mod incoming;
mod init;
mod reply;
mod signal;
mod stored;
mod user;

pub use common::{Dispatch, Message, MessageDetails, ReplyDetails, SignalDetails};
pub use context::{
    ContextOutcome, ContextOutcomeDrain, ContextSettings, ContextStore, MessageContext,
};
pub use gear_core_errors::{ErrorReplyReason, ReplyCode, SuccessReplyReason};
pub use handle::{HandleMessage, HandlePacket};
pub use incoming::{IncomingDispatch, IncomingMessage};
pub use init::{InitMessage, InitPacket};
pub use reply::{ReplyMessage, ReplyPacket};
pub use signal::SignalMessage;
pub use stored::{StoredDelayedDispatch, StoredDispatch, StoredMessage};
pub use user::{UserMessage, UserStoredMessage};

use core::fmt::Debug;
use gear_wasm_instrument::syscalls::SyscallName;
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

/// Gas limit type for message.
pub type GasLimit = u64;

/// Value type for message.
pub type Value = u128;

/// Salt type for init message.
pub type Salt = crate::buffer::Payload;

/// Entry point for dispatch processing.
#[derive(
    Copy, Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum DispatchKind {
    /// Initialization.
    Init,
    /// Common handle.
    #[default]
    Handle,
    /// Handle reply.
    Reply,
    /// System signal.
    Signal,
}

impl DispatchKind {
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

    /// Returns is syscall forbidden for the dispatch kind.
    pub fn forbids(&self, syscall_name: SyscallName) -> bool {
        match self {
            DispatchKind::Signal => matches!(
                syscall_name,
                SyscallName::Source
                    | SyscallName::Reply
                    | SyscallName::ReplyPush
                    | SyscallName::ReplyCommit
                    | SyscallName::ReplyCommitWGas
                    | SyscallName::ReplyInput
                    | SyscallName::ReplyInputWGas
                    | SyscallName::ReservationReply
                    | SyscallName::ReservationReplyCommit
                    | SyscallName::SystemReserveGas
            ),
            _ => false,
        }
    }
}

/// Message packet.
///
/// Provides common behavior for any message's packet: accessing to payload, gas limit and value.
pub trait Packet {
    /// Packet payload bytes.
    fn payload_bytes(&self) -> &[u8];

    /// Payload len
    fn payload_len(&self) -> u32;

    /// Packet optional gas limit.
    fn gas_limit(&self) -> Option<GasLimit>;

    /// Packet value.
    fn value(&self) -> Value;

    /// A dispatch kind the will be generated from the packet.
    fn kind() -> DispatchKind;
}
