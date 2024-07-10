// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! Type definitions and helpers for error handling.
//!
//! Enumerates possible errors in programs `Error`.
//! Errors related to conversion, decoding, message status code, other internal
//! errors.

use alloc::vec::Vec;
use core::{fmt, str};

pub use gcore::errors::{Error as CoreError, *};
pub use scale_info::scale::Error as CodecError;

/// `Result` type with a predefined error type ([`Error`]).
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Common error type returned by API functions from other modules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /* Protocol under-hood errors */
    /// Error type from `gcore`.
    ///
    /// NOTE: this error could only be returned from syscalls.
    Core(CoreError),

    /* API lib under-hood errors */
    /// Conversion error.
    ///
    /// NOTE: this error returns from incorrect bytes conversion.
    Convert(ConversionError),

    /// `scale-codec` decoding error.
    ///
    /// NOTE: this error returns from APIs that return specific `Decode` types.
    Decode(CodecError),

    /// Gstd API usage error.
    ///
    /// Note: this error returns from `gstd` APIs in case of invalid arguments.
    Gstd(UsageError),

    /* Business logic errors */
    /// Received error reply while awaited response from another actor.
    ///
    /// NOTE: this error could only be returned from async messaging.
    ErrorReply(ErrorReplyPayload, ErrorReplyReason),

    /// Received reply that couldn't be identified as successful or not
    /// due to unsupported reply code.
    ///
    /// NOTE: this error could only be returned from async messaging.
    UnsupportedReply(Vec<u8>),

    /// Timeout reached while expecting for reply.
    ///
    /// NOTE: this error could only be returned from async messaging.
    Timeout(u32, u32),
}

impl Error {
    /// Check whether an error is [`Error::Timeout`].
    pub fn timed_out(&self) -> bool {
        matches!(self, Error::Timeout(..))
    }

    /// Check whether an error is [`Error::ErrorReply`] and return its str
    /// representation.
    pub fn error_reply_str(&self) -> Option<&str> {
        if let Self::ErrorReply(payload, _) = self {
            payload.try_as_str()
        } else {
            None
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Core(e) => fmt::Display::fmt(e, f),
            Error::Convert(e) => write!(f, "Conversion error: {e:?}"),
            Error::Decode(e) => write!(f, "Scale codec decoding error: {e}"),
            Error::Gstd(e) => write!(f, "`Gstd` API error: {e:?}"),
            Error::ErrorReply(err, reason) => write!(f, "Received reply '{err}' due to {reason:?}"),
            Error::UnsupportedReply(payload) => {
                write!(f, "Received unsupported reply '0x{}'", hex::encode(payload))
            }
            Error::Timeout(expected, now) => {
                write!(f, "Timeout has occurred: expected at {expected}, now {now}")
            }
        }
    }
}

impl From<CoreError> for Error {
    fn from(err: CoreError) -> Self {
        Self::Core(err)
    }
}

impl From<ConversionError> for Error {
    fn from(err: ConversionError) -> Self {
        Self::Convert(err)
    }
}

/// New-type representing error reply payload. Expected to be utf-8 string.
#[derive(Clone, Eq, PartialEq)]
pub struct ErrorReplyPayload(pub Vec<u8>);

impl ErrorReplyPayload {
    /// Represents self as utf-8 str, if possible.
    pub fn try_as_str(&self) -> Option<&str> {
        str::from_utf8(&self.0).ok()
    }

    /// Similar to [`Self::try_as_str`], but panics in `None` case.
    /// Preferable to use only for test purposes.
    #[track_caller]
    pub fn as_str(&self) -> &str {
        str::from_utf8(&self.0).expect("Failed to create `str`")
    }
}

impl From<Vec<u8>> for ErrorReplyPayload {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

impl fmt::Debug for ErrorReplyPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.try_as_str()
            .map(|v| write!(f, "{v}"))
            .unwrap_or_else(|| write!(f, "0x{}", hex::encode(&self.0)))
    }
}

impl fmt::Display for ErrorReplyPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Error type returned by gstd API while using invalid arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UsageError {
    /// This error occurs when providing zero duration to waiting functions
    /// (e.g. see `exactly` and `up_to` functions in
    /// [`CodecMessageFuture`](crate::msg::CodecMessageFuture)).
    EmptyWaitDuration,
    /// This error occurs when providing zero gas amount to system gas reserving
    /// function (see
    /// [`Config::set_system_reserve`](crate::Config::set_system_reserve)).
    ZeroSystemReservationAmount,
    /// This error occurs when providing zero duration to mutex lock function
    ZeroMxLockDuration,
    /// This error occurs when handle_reply is called without (or with zero)
    /// reply deposit
    /// (see [`MessageFuture::handle_reply`](crate::msg::MessageFuture::handle_reply)).
    ZeroReplyDeposit,
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsageError::EmptyWaitDuration => write!(f, "Wait duration can not be zero"),
            UsageError::ZeroSystemReservationAmount => {
                write!(f, "System reservation amount can not be zero in config")
            }
            UsageError::ZeroMxLockDuration => write!(f, "Mutex lock duration can not be zero"),
            UsageError::ZeroReplyDeposit => write!(f, "Reply deposit can not be zero"),
        }
    }
}
