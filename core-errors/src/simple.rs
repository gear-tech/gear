// // Copyright (C) 2023 Gear Technologies Inc.
// // SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// //
// // This program is free software: you can redistribute it and/or modify
// // it under the terms of the GNU General Public License as published by
// // the Free Software Foundation, either version 3 of the License, or
// // (at your option) any later version.
// //
// // This program is distributed in the hope that it will be useful,
// // but WITHOUT ANY WARRANTY; without even the implied warranty of
// // MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// // GNU General Public License for more details.
// //
// // You should have received a copy of the GNU General Public License
// // along with this program. If not, see <https://www.gnu.org/licenses/>.

// //! Simple errors being used for status codes

#![allow(missing_docs)]
#![allow(clippy::unnecessary_cast)]

#[cfg(feature = "codec")]
use scale_info::{
    scale::{self, Decode, Encode},
    TypeInfo,
};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale))]
pub enum ReplyCode {
    Success(SuccessReason) = 0,
    Error(ErrorReason) = 1,
    //
    #[default]
    Unsupported = 255,
}

impl From<SuccessReason> for ReplyCode {
    fn from(reason: SuccessReason) -> Self {
        Self::Success(reason)
    }
}

impl From<ErrorReason> for ReplyCode {
    fn from(reason: ErrorReason) -> Self {
        Self::Error(reason)
    }
}

impl ReplyCode {
    pub fn to_bytes(self) -> [u8; 4] {
        // SAFETY: Because `Self` is marked `repr(u8)`, its layout is a `repr(C)` `union`
        // between `repr(C)` structs, each of which has the `u8` discriminant as its first
        // field, so we can read the discriminant without offsetting the pointer.
        let first = unsafe { *<*const _>::from(&self).cast::<u8>() };

        let mut bytes = [first, 0, 0, 0];

        match self {
            Self::Success(reason) => bytes[1..].copy_from_slice(&reason.to_bytes()),
            Self::Error(reason) => bytes[1..].copy_from_slice(&reason.to_bytes()),
            Self::Unsupported => {}
        }

        bytes
    }

    pub fn from_bytes(bytes: [u8; 4]) -> Self {
        match bytes[0] {
            0 => {
                let reason_bytes = bytes[1..].try_into().unwrap_or_else(|_| unreachable!());
                Self::Success(SuccessReason::from_bytes(reason_bytes))
            }
            1 => {
                let reason_bytes = bytes[1..].try_into().unwrap_or_else(|_| unreachable!());
                Self::Error(ErrorReason::from_bytes(reason_bytes))
            }
            _ => Self::Unsupported,
        }
    }

    pub fn error(reason: impl Into<ErrorReason>) -> Self {
        Self::Error(reason.into())
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported)
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale))]
pub enum SuccessReason {
    Auto = 0,
    Manual = 1,
    //
    #[default]
    Unsupported = 255,
}

impl SuccessReason {
    fn to_bytes(self) -> [u8; 3] {
        [self as u8, 0, 0]
    }

    fn from_bytes(bytes: [u8; 3]) -> Self {
        match bytes[0] {
            0 => Self::Auto,
            1 => Self::Manual,
            _ => Self::Unsupported,
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale))]
pub enum ErrorReason {
    Execution(ExecutionError) = 0,
    InactiveProgram = 1,
    FailedToCreateProgram = 2,
    RemovedFromWaitlist = 3,
    //
    #[default]
    Unsupported = 255,
}

impl From<ExecutionError> for ErrorReason {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}

impl ErrorReason {
    fn to_bytes(self) -> [u8; 3] {
        // SAFETY: Because `Self` is marked `repr(u8)`, its layout is a `repr(C)` `union`
        // between `repr(C)` structs, each of which has the `u8` discriminant as its first
        // field, so we can read the discriminant without offsetting the pointer.
        let first = unsafe { *<*const _>::from(&self).cast::<u8>() };

        let mut bytes = [first, 0, 0];

        match self {
            Self::Execution(error) => {
                bytes[1..].copy_from_slice(&error.to_bytes());
            }
            Self::InactiveProgram
            | Self::FailedToCreateProgram
            | Self::RemovedFromWaitlist
            | Self::Unsupported => {}
        }

        bytes
    }

    fn from_bytes(bytes: [u8; 3]) -> Self {
        match bytes[0] {
            0 => {
                let err_bytes = bytes[1..].try_into().unwrap_or_else(|_| unreachable!());
                Self::Execution(ExecutionError::from_bytes(err_bytes))
            }
            1 => Self::InactiveProgram,
            2 => Self::FailedToCreateProgram,
            _ => Self::Unsupported,
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale))]
pub enum ExecutionError {
    RanOutOfGas = 0,
    MemoryOverflow = 1,
    BackendError = 2,
    UserspacePanic = 3,
    UnreachableInstruction = 4,
    //
    #[default]
    Unsupported = 255,
}

impl ExecutionError {
    fn to_bytes(self) -> [u8; 2] {
        [self as u8, 0]
    }

    fn from_bytes(bytes: [u8; 2]) -> Self {
        match bytes[0] {
            0 => Self::RanOutOfGas,
            1 => Self::MemoryOverflow,
            2 => Self::BackendError,
            3 => Self::UserspacePanic,
            4 => Self::UnreachableInstruction,
            _ => Self::Unsupported,
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "codec", derive(Encode, Decode, TypeInfo), codec(crate = scale))]
pub enum SignalCode {
    Execution(ExecutionError),
    RemovedFromWaitlist,
    //
    #[default]
    Unsupported,
}

impl From<ExecutionError> for SignalCode {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}
