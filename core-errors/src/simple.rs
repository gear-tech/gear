// Copyright (C) 2023-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Simple errors being used for status codes

use enum_iterator::Sequence;
#[cfg(feature = "codec")]
use scale_info::{
    scale::{self, Decode, Encode},
    TypeInfo,
};

#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    derive_more::Display,
    derive_more::From,
    Sequence,
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale), allow(clippy::unnecessary_cast))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Enum representing reply code with reason of its creation.
pub enum ReplyCode {
    /// Success reply.
    #[display(fmt = "Success reply sent due to {_0}")]
    Success(SuccessReplyReason) = 0,

    /// Error reply.
    #[display(fmt = "Error reply sent due to {_0}")]
    Error(ErrorReplyReason) = 1,

    /// Unsupported code.
    /// Variant exists for backward compatibility.
    #[default]
    #[display(fmt = "<unsupported reply code>")]
    Unsupported = 255,
}

impl ReplyCode {
    fn discriminant(&self) -> u8 {
        // SAFETY: Because `Self` is marked `repr(u8)`, its layout is a `repr(C)` `union`
        // between `repr(C)` structs, each of which has the `u8` discriminant as its first
        // field, so we can read the discriminant without offsetting the pointer.
        unsafe { *<*const _>::from(self).cast::<u8>() }
    }

    /// Converts `ReplyCode` to 4 bytes array.
    pub fn to_bytes(self) -> [u8; 4] {
        let mut bytes = [self.discriminant(), 0, 0, 0];

        match self {
            Self::Success(reason) => bytes[1..].copy_from_slice(&reason.to_bytes()),
            Self::Error(reason) => bytes[1..].copy_from_slice(&reason.to_bytes()),
            Self::Unsupported => {}
        }

        bytes
    }

    /// Parses 4 bytes array to `ReplyCode`.
    pub fn from_bytes(bytes: [u8; 4]) -> Self {
        match bytes[0] {
            b if Self::Success(Default::default()).discriminant() == b => {
                let reason_bytes = bytes[1..].try_into().unwrap_or_else(|_| unreachable!());
                Self::Success(SuccessReplyReason::from_bytes(reason_bytes))
            }
            b if Self::Error(Default::default()).discriminant() == b => {
                let reason_bytes = bytes[1..].try_into().unwrap_or_else(|_| unreachable!());
                Self::Error(ErrorReplyReason::from_bytes(reason_bytes))
            }
            _ => Self::Unsupported,
        }
    }

    /// Constructs `ReplyCode::Error(_)` variant from underlying reason.
    pub fn error(reason: impl Into<ErrorReplyReason>) -> Self {
        Self::Error(reason.into())
    }

    /// Returns bool, defining if `ReplyCode` represents success reply.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    /// Returns bool, defining if `ReplyCode` represents error reply.
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    /// Returns bool, defining if `ReplyCode` represents unsupported reason.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported)
    }
}

#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    derive_more::Display,
    Sequence,
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale), allow(clippy::unnecessary_cast))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Reason of success reply creation.
pub enum SuccessReplyReason {
    /// Success reply was created by system automatically.
    #[display(fmt = "automatic sending")]
    Auto = 0,

    /// Success reply was created by actor manually.
    #[display(fmt = "manual sending")]
    Manual = 1,

    /// Unsupported reason of success reply.
    /// Variant exists for backward compatibility.
    #[default]
    #[display(fmt = "<unsupported reason>")]
    Unsupported = 255,
}

impl SuccessReplyReason {
    fn to_bytes(self) -> [u8; 3] {
        [self as u8, 0, 0]
    }

    fn from_bytes(bytes: [u8; 3]) -> Self {
        match bytes[0] {
            b if Self::Auto as u8 == b => Self::Auto,
            b if Self::Manual as u8 == b => Self::Manual,
            _ => Self::Unsupported,
        }
    }
}

#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    derive_more::Display,
    derive_more::From,
    Sequence,
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale), allow(clippy::unnecessary_cast))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Reason of error reply creation.
// NOTE: Adding new variants to this enum you must also update `ErrorReplyReason::to_bytes` and
// `ErrorReplyReason::from_bytes` methods.
pub enum ErrorReplyReason {
    /// Error reply was created due to underlying execution error.
    #[display(fmt = "execution error ({_0})")]
    Execution(SimpleExecutionError) = 0,

    /// Destination actor is unavailable, so it can't process the message.
    #[display(fmt = "destination actor is unavailable ({_0})")]
    UnavailableActor(SimpleUnavailableActorError) = 2,

    /// Message has died in Waitlist as out of rent one.
    #[display(fmt = "removal from waitlist")]
    RemovedFromWaitlist = 3,

    /// Unsupported reason of error reply.
    /// Variant exists for backward compatibility.
    #[default]
    #[display(fmt = "<unsupported reason>")]
    Unsupported = 255,
}

impl ErrorReplyReason {
    /// Returns bool indicating if self is UnavailableActor::ProgramExited variant.
    pub fn is_exited(&self) -> bool {
        matches!(
            self,
            Self::UnavailableActor(SimpleUnavailableActorError::ProgramExited)
        )
    }

    /// Returns bool indicating if self is Execution::UserspacePanic variant.
    pub fn is_userspace_panic(&self) -> bool {
        matches!(self, Self::Execution(SimpleExecutionError::UserspacePanic))
    }

    fn discriminant(&self) -> u8 {
        // SAFETY: Because `Self` is marked `repr(u8)`, its layout is a `repr(C)` `union`
        // between `repr(C)` structs, each of which has the `u8` discriminant as its first
        // field, so we can read the discriminant without offsetting the pointer.
        unsafe { *<*const _>::from(self).cast::<u8>() }
    }

    fn to_bytes(self) -> [u8; 3] {
        let mut bytes = [self.discriminant(), 0, 0];

        match self {
            Self::Execution(error) => bytes[1..].copy_from_slice(&error.to_bytes()),
            Self::UnavailableActor(error) => bytes[1..].copy_from_slice(&error.to_bytes()),
            Self::RemovedFromWaitlist | Self::Unsupported => {}
        }

        bytes
    }

    fn from_bytes(bytes: [u8; 3]) -> Self {
        match bytes[0] {
            1 /* removed `FailedToCreateProgram` variant */ |
            4 /* moved `ReinstrumentationFailure` variant */ => Self::Unsupported,
            b if Self::Execution(Default::default()).discriminant() == b => {
                let err_bytes = bytes[1..].try_into().unwrap_or_else(|_| unreachable!());
                Self::Execution(SimpleExecutionError::from_bytes(err_bytes))
            }
            b if Self::UnavailableActor(Default::default()).discriminant() == b => {
                let err_bytes = bytes[1..].try_into().unwrap_or_else(|_| unreachable!());
                Self::UnavailableActor(SimpleUnavailableActorError::from_bytes(err_bytes))
            }
            b if Self::RemovedFromWaitlist.discriminant() == b => Self::RemovedFromWaitlist,
            _ => Self::Unsupported,
        }
    }
}

#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    derive_more::Display,
    Sequence,
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale), allow(clippy::unnecessary_cast))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Simplified error occurred during execution.
pub enum SimpleExecutionError {
    /// Message ran out of gas while executing.
    #[display(fmt = "Message ran out of gas")]
    RanOutOfGas = 0,

    /// Program has reached memory limit while executing.
    #[display(fmt = "Program reached memory limit")]
    MemoryOverflow = 1,

    /// Execution failed with backend error that couldn't been caught.
    #[display(fmt = "Message ran into uncatchable error")]
    BackendError = 2,

    /// Execution failed with userspace panic.
    ///
    /// **PAYLOAD**: Arbitrary payload given by the program as `gr_panic` argument.
    #[display(fmt = "Message panicked")]
    UserspacePanic = 3,

    /// Execution failed with `unreachable` instruction call.
    #[display(fmt = "Program called WASM `unreachable` instruction")]
    UnreachableInstruction = 4,

    /// Program has reached stack limit while executing.
    #[display(fmt = "Program reached stack limit")]
    StackLimitExceeded = 5,

    /// Unsupported reason of execution error.
    /// Variant exists for backward compatibility.
    #[default]
    #[display(fmt = "<unsupported error>")]
    Unsupported = 255,
}

impl SimpleExecutionError {
    fn to_bytes(self) -> [u8; 2] {
        [self as u8, 0]
    }

    fn from_bytes(bytes: [u8; 2]) -> Self {
        match bytes[0] {
            b if Self::RanOutOfGas as u8 == b => Self::RanOutOfGas,
            b if Self::MemoryOverflow as u8 == b => Self::MemoryOverflow,
            b if Self::BackendError as u8 == b => Self::BackendError,
            b if Self::UserspacePanic as u8 == b => Self::UserspacePanic,
            b if Self::UnreachableInstruction as u8 == b => Self::UnreachableInstruction,
            b if Self::StackLimitExceeded as u8 == b => Self::StackLimitExceeded,
            _ => Self::Unsupported,
        }
    }
}

#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    derive_more::Display,
    Sequence,
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale), allow(clippy::unnecessary_cast))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Simplified error occurred because of actor unavailability.
pub enum SimpleUnavailableActorError {
    /// Program called `gr_exit` syscall.
    ///
    /// **PAYLOAD**: `ActorId` of the exited program's inheritor (`gr_exit` argument).
    #[display(fmt = "Program exited")]
    ProgramExited = 0,

    /// Program was terminated due to failed initialization.
    #[display(fmt = "Program was terminated due failed initialization")]
    InitializationFailure = 1,

    /// Program is not initialized yet.
    #[display(fmt = "Program is not initialized yet")]
    Uninitialized = 2,

    /// Program was not created.
    #[display(fmt = "Program was not created")]
    ProgramNotCreated = 3,

    /// Program re-instrumentation failed.
    #[display(fmt = "Program re-instrumentation failed")]
    ReinstrumentationFailure = 4,

    /// Unsupported reason of inactive actor error.
    /// Variant exists for backward compatibility.
    #[default]
    #[display(fmt = "<unsupported error>")]
    Unsupported = 255,
}

impl SimpleUnavailableActorError {
    fn to_bytes(self) -> [u8; 2] {
        [self as u8, 0]
    }

    fn from_bytes(bytes: [u8; 2]) -> Self {
        match bytes[0] {
            b if Self::ProgramExited as u8 == b => Self::ProgramExited,
            b if Self::InitializationFailure as u8 == b => Self::InitializationFailure,
            b if Self::Uninitialized as u8 == b => Self::Uninitialized,
            b if Self::ProgramNotCreated as u8 == b => Self::ProgramNotCreated,
            b if Self::ReinstrumentationFailure as u8 == b => Self::ReinstrumentationFailure,
            _ => Self::Unsupported,
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    derive_more::Display,
    derive_more::From,
    Sequence,
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale), allow(clippy::unnecessary_cast))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Enum representing signal code and reason of its creation.
///
/// # Testing
/// See [this document](../signal-code-testing.md).
pub enum SignalCode {
    /// Signal was sent due to some execution errors.
    #[display(fmt = "Signal message sent due to execution error ({_0})")]
    Execution(SimpleExecutionError),

    /// Signal was sent due to removal from waitlist as out of rent.
    #[default]
    #[display(fmt = "Signal message sent due to removal from waitlist")]
    RemovedFromWaitlist,
}

impl SignalCode {
    /// Converts `SignalCode` into `u32`.
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Execution(SimpleExecutionError::UserspacePanic) => 100,
            Self::Execution(SimpleExecutionError::RanOutOfGas) => 101,
            Self::Execution(SimpleExecutionError::BackendError) => 102,
            Self::Execution(SimpleExecutionError::MemoryOverflow) => 103,
            Self::Execution(SimpleExecutionError::UnreachableInstruction) => 104,
            Self::Execution(SimpleExecutionError::StackLimitExceeded) => 105,
            Self::RemovedFromWaitlist => 200,
            // Must be unreachable.
            Self::Execution(SimpleExecutionError::Unsupported) => u32::MAX,
        }
    }

    /// Parses `SignalCode` from `u32` if possible.
    pub const fn from_u32(num: u32) -> Option<Self> {
        let res = match num {
            v if Self::Execution(SimpleExecutionError::UserspacePanic).to_u32() == v => {
                Self::Execution(SimpleExecutionError::UserspacePanic)
            }
            v if Self::Execution(SimpleExecutionError::RanOutOfGas).to_u32() == v => {
                Self::Execution(SimpleExecutionError::RanOutOfGas)
            }
            v if Self::Execution(SimpleExecutionError::BackendError).to_u32() == v => {
                Self::Execution(SimpleExecutionError::BackendError)
            }
            v if Self::Execution(SimpleExecutionError::MemoryOverflow).to_u32() == v => {
                Self::Execution(SimpleExecutionError::MemoryOverflow)
            }
            v if Self::Execution(SimpleExecutionError::UnreachableInstruction).to_u32() == v => {
                Self::Execution(SimpleExecutionError::UnreachableInstruction)
            }
            v if Self::Execution(SimpleExecutionError::StackLimitExceeded).to_u32() == v => {
                Self::Execution(SimpleExecutionError::StackLimitExceeded)
            }
            v if Self::Execution(SimpleExecutionError::Unsupported).to_u32() == v => {
                Self::Execution(SimpleExecutionError::Unsupported)
            }
            v if Self::RemovedFromWaitlist.to_u32() == v => Self::RemovedFromWaitlist,
            _ => return None,
        };

        Some(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forbidden_codes() {
        let codes = [
            1, // `FailedToCreateProgram` variant
            4, // `ReinstrumentationFailure` variant
        ];

        // check forbidden code is `Unsupported` variant now
        for code in codes {
            let err = ErrorReplyReason::from_bytes([code, 0, 0]);
            assert_eq!(err, ErrorReplyReason::Unsupported);
        }

        // check forbidden code is never produced
        for code in enum_iterator::all::<ErrorReplyReason>() {
            let bytes = code.to_bytes();
            assert!(!codes.contains(&bytes[0]));
        }
    }

    #[test]
    fn test_reply_code_encode_decode() {
        for code in enum_iterator::all::<ReplyCode>() {
            let bytes = code.to_bytes();
            assert_eq!(code, ReplyCode::from_bytes(bytes));
        }
    }

    #[test]
    fn test_signal_code_encode_decode() {
        for signal in enum_iterator::all::<SignalCode>() {
            let code = signal.to_u32();
            assert_eq!(signal, SignalCode::from_u32(code).unwrap());
        }
    }
}
