// Copyright (C) 2023 Gear Technologies Inc.
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

#[cfg(feature = "codec")]
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
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo, Sequence), codec(crate = scale), allow(clippy::unnecessary_cast))]
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
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, derive_more::Display,
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo, Sequence), codec(crate = scale), allow(clippy::unnecessary_cast))]
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
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo, Sequence), codec(crate = scale), allow(clippy::unnecessary_cast))]
/// Reason of error reply creation.
pub enum ErrorReplyReason {
    /// Error reply was created due to underlying execution error.
    #[display(fmt = "execution error ({_0})")]
    Execution(SimpleExecutionError) = 0,

    /// Error reply was created due to errors in program creation.
    #[display(fmt = "fail in program creation ({_0})")]
    FailedToCreateProgram(SimpleProgramCreationError) = 1,

    /// Destination actor become inactive program and can't process the message.
    #[display(fmt = "inactivity of destination program")]
    InactiveProgram = 2,

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
            Self::FailedToCreateProgram(error) => bytes[1..].copy_from_slice(&error.to_bytes()),
            Self::InactiveProgram | Self::RemovedFromWaitlist | Self::Unsupported => {}
        }

        bytes
    }

    fn from_bytes(bytes: [u8; 3]) -> Self {
        match bytes[0] {
            b if Self::Execution(Default::default()).discriminant() == b => {
                let err_bytes = bytes[1..].try_into().unwrap_or_else(|_| unreachable!());
                Self::Execution(SimpleExecutionError::from_bytes(err_bytes))
            }
            b if Self::FailedToCreateProgram(Default::default()).discriminant() == b => {
                let err_bytes = bytes[1..].try_into().unwrap_or_else(|_| unreachable!());
                Self::FailedToCreateProgram(SimpleProgramCreationError::from_bytes(err_bytes))
            }
            b if Self::InactiveProgram.discriminant() == b => Self::InactiveProgram,
            b if Self::RemovedFromWaitlist.discriminant() == b => Self::RemovedFromWaitlist,
            _ => Self::Unsupported,
        }
    }
}

#[repr(u8)]
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, derive_more::Display,
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo, Sequence), codec(crate = scale), allow(clippy::unnecessary_cast))]
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
    #[display(fmt = "Message panicked")]
    UserspacePanic = 3,

    /// Execution failed with `unreachable` instruction call.
    #[display(fmt = "Program called WASM `unreachable` instruction")]
    UnreachableInstruction = 4,

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
            _ => Self::Unsupported,
        }
    }
}

#[repr(u8)]
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, derive_more::Display,
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo, Sequence), codec(crate = scale), allow(clippy::unnecessary_cast))]
/// Simplified error occurred during program creation.
pub enum SimpleProgramCreationError {
    /// Given code id for program creation doesn't exist.
    #[display(fmt = "Given `CodeId` doesn't exist")]
    CodeNotExists = 0,

    // -----
    // TODO: consider should such error appear or not #2821.
    // /// Resulting program id for program creation already exists.
    // ProgramIdAlreadyExists = 1,
    // -----
    /// Unsupported reason of program creation error.
    /// Variant exists for backward compatibility.
    #[default]
    #[display(fmt = "<unsupported error>")]
    Unsupported = 255,
}

impl SimpleProgramCreationError {
    fn to_bytes(self) -> [u8; 2] {
        [self as u8, 0]
    }

    fn from_bytes(bytes: [u8; 2]) -> Self {
        match bytes[0] {
            b if Self::CodeNotExists as u8 == b => Self::CodeNotExists,
            // TODO: #2821
            // b if Self::ProgramIdAlreadyExists as u8 == b => Self::ProgramIdAlreadyExists,
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
)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo, Sequence), codec(crate = scale), allow(clippy::unnecessary_cast))]
/// Enum representing signal code and reason of its creation.
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
            v if Self::RemovedFromWaitlist.to_u32() == v => Self::RemovedFromWaitlist,
            _ => return None,
        };

        Some(res)
    }
}
