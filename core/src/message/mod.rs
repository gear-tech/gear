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
use parity_scale_codec::{Compact, MaxEncodedLen};
pub use reply::{ReplyMessage, ReplyPacket};
pub use signal::SignalMessage;
pub use stored::{StoredDelayedDispatch, StoredDispatch, StoredMessage};
pub use user::{UserMessage, UserStoredMessage};

use super::buffer::LimitedVec;
use crate::str::LimitedStr;
use alloc::string::String;
use core::fmt::{Debug, Display};
use gear_wasm_instrument::syscalls::SyscallName;
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

/// Max payload size which one message can have (8 MiB).
pub const MAX_PAYLOAD_SIZE: usize = 8 * 1024 * 1024;

// **WARNING**: do not remove this check
const _: () = assert!(MAX_PAYLOAD_SIZE <= u32::MAX as usize);

/// Payload size exceed error
#[derive(
    Clone, Copy, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
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

impl Payload {
    /// Get payload length as u32.
    pub fn len_u32(&self) -> u32 {
        // Safe, cause it's guarantied: `MAX_PAYLOAD_SIZE` <= u32::MAX
        self.inner().len() as u32
    }
}

impl MaxEncodedLen for Payload {
    fn max_encoded_len() -> usize {
        Compact::<u32>::max_encoded_len() + MAX_PAYLOAD_SIZE
    }
}

/// Panic buffer which size cannot be bigger then max allowed payload size.
#[derive(
    Clone,
    Default,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Decode,
    Encode,
    TypeInfo,
    derive_more::From,
    derive_more::Into,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct PanicBuffer(Payload);

impl PanicBuffer {
    /// Returns ref to the internal data.
    pub fn inner(&self) -> &Payload {
        &self.0
    }

    fn to_limited_str(&self) -> Option<LimitedStr> {
        let s = core::str::from_utf8(self.0.inner()).ok()?;
        LimitedStr::try_from(s).ok()
    }
}

impl From<LimitedStr<'_>> for PanicBuffer {
    fn from(value: LimitedStr) -> Self {
        const _: () = assert!(crate::str::TRIMMED_MAX_LEN <= MAX_PAYLOAD_SIZE);
        Payload::try_from(value.into_inner().into_owned().into_bytes())
            .map(Self)
            .unwrap_or_else(|PayloadSizeError| {
                unreachable!("`LimitedStr` is always smaller than maximum payload size",)
            })
    }
}

impl Display for PanicBuffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(s) = self.to_limited_str() {
            Display::fmt(&s, f)
        } else {
            Display::fmt(&self.0, f)
        }
    }
}

impl Debug for PanicBuffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(s) = self.to_limited_str() {
            Debug::fmt(s.as_str(), f)
        } else {
            Debug::fmt(&self.0, f)
        }
    }
}

/// Gas limit type for message.
pub type GasLimit = u64;

/// Value type for message.
pub type Value = u128;

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

/// Trait defining type could be used as entry point for a wasm module.
pub trait WasmEntryPoint: Sized {
    /// Converting self into entry point name.
    fn as_entry(&self) -> &str;

    /// Converting entry point name into self object, if possible.
    fn try_from_entry(entry: &str) -> Option<Self>;

    /// Tries to convert self into `DispatchKind`.
    fn try_into_kind(&self) -> Option<DispatchKind> {
        <DispatchKind as WasmEntryPoint>::try_from_entry(self.as_entry())
    }
}

impl WasmEntryPoint for String {
    fn as_entry(&self) -> &str {
        self
    }

    fn try_from_entry(entry: &str) -> Option<Self> {
        Some(entry.into())
    }
}

impl WasmEntryPoint for DispatchKind {
    fn as_entry(&self) -> &str {
        match self {
            Self::Init => "init",
            Self::Handle => "handle",
            Self::Reply => "handle_reply",
            Self::Signal => "handle_signal",
        }
    }

    fn try_from_entry(entry: &str) -> Option<Self> {
        let kind = match entry {
            "init" => Self::Init,
            "handle" => Self::Handle,
            "handle_reply" => Self::Reply,
            "handle_signal" => Self::Signal,
            _ => return None,
        };

        Some(kind)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    fn panic_buf(bytes: &[u8]) -> PanicBuffer {
        Payload::try_from(bytes).map(PanicBuffer).unwrap()
    }

    #[test]
    fn panic_buffer_debug() {
        let buf = panic_buf(b"Hello, world!");
        assert_eq!(format!("{buf:?}"), r#""Hello, world!""#);

        let buf = panic_buf(b"\xE0\x80\x80");
        assert_eq!(format!("{buf:?}"), "0xe08080");
    }

    #[test]
    fn panic_buffer_display() {
        let buf = panic_buf(b"Hello, world!");
        assert_eq!(format!("{buf}"), "Hello, world!");

        let buf = panic_buf(b"\xE0\x80\x80");
        assert_eq!(format!("{buf}"), "0xe08080");
    }
}
