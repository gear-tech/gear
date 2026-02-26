// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

pub use gcore::errors::{Error as CoreError, *};
pub use scale_info::scale::Error as CodecError;

use crate::ActorId;
use alloc::vec::Vec;
use core::{fmt, str};
use gprimitives::utils::ByteSliceFormatter;
use parity_scale_codec::Decode;

/// `Result` type with a predefined error type ([`Error`]).
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Common error type returned by API functions from other modules.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /* Protocol under-hood errors */
    /// Error type from `gcore`.
    ///
    /// NOTE: this error could only be returned from syscalls.
    #[error(transparent)]
    Core(#[from] CoreError),

    /* API lib under-hood errors */
    /// Conversion error.
    ///
    /// NOTE: this error returns from incorrect bytes conversion.
    #[error("Conversion error: {0}")]
    Convert(#[from] ConversionError),

    /// `scale-codec` decoding error.
    ///
    /// NOTE: this error returns from APIs that return specific `Decode` types.
    #[error("Scale codec decoding error: {0}")]
    Decode(CodecError),

    /// Gstd API usage error.
    ///
    /// Note: this error returns from `gstd` APIs in case of invalid arguments.
    #[error("Gstd API error: {0}")]
    Gstd(#[from] UsageError),

    /* Business logic errors */
    /// Received error reply while awaited response from another actor.
    ///
    /// NOTE: this error could only be returned from async messaging.
    // TODO: consider to load payload lazily (#4595)
    #[error("Received error reply '{0}' due to {1}")]
    ErrorReply(ErrorReplyPayload, ErrorReplyReason),

    /// Received reply that couldn't be identified as successful or not
    /// due to unsupported reply code.
    ///
    /// NOTE: this error could only be returned from async messaging.
    #[error("Received unsupported reply '{hex}'", hex = ByteSliceFormatter::Dynamic(.0))]
    UnsupportedReply(Vec<u8>),

    /// Timeout reached while expecting for reply.
    ///
    /// NOTE: this error could only be returned from async messaging.
    #[error("Timeout has occurred: expected at {0}, now {1}")]
    Timeout(u32, u32),
}

impl Error {
    /// Check whether an error is [`Error::Timeout`].
    pub fn timed_out(&self) -> bool {
        matches!(self, Error::Timeout(..))
    }

    /// Check whether an error is [`SimpleExecutionError::UserspacePanic`] from
    /// error reply and return its decoded message's payload of
    /// a custom type.
    pub fn error_reply_panic<T: Decode>(&self) -> Option<Result<T, Self>> {
        self.error_reply_panic_bytes()
            .map(|mut bytes| T::decode(&mut bytes).map_err(Error::Decode))
    }

    /// Check whether an error is [`SimpleExecutionError::UserspacePanic`] from
    /// error reply and return its payload as bytes.
    pub fn error_reply_panic_bytes(&self) -> Option<&[u8]> {
        if let Self::ErrorReply(
            payload,
            ErrorReplyReason::Execution(SimpleExecutionError::UserspacePanic),
        ) = self
        {
            Some(&payload.0)
        } else {
            None
        }
    }

    /// Check whether an error is [`SimpleUnavailableActorError::ProgramExited`]
    /// from error reply and return inheritor of exited program.
    pub fn error_reply_exit_inheritor(&self) -> Option<ActorId> {
        if let Self::ErrorReply(
            payload,
            ErrorReplyReason::UnavailableActor(SimpleUnavailableActorError::ProgramExited),
        ) = self
        {
            let id = ActorId::try_from(payload.0.as_slice())
                .unwrap_or_else(|e| unreachable!("protocol always returns valid `ActorId`: {e}"));
            Some(id)
        } else {
            None
        }
    }
}

/// New-type representing error reply payload. Expected to be utf-8 string.
#[derive(Clone, Eq, PartialEq)]
pub struct ErrorReplyPayload(pub Vec<u8>);

impl ErrorReplyPayload {
    /// Returns byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// Represents self as utf-8 str, if possible.
    pub fn try_as_str(&self) -> Option<&str> {
        str::from_utf8(&self.0).ok()
    }

    /// Returns inner byte vector.
    pub fn into_inner(self) -> Vec<u8> {
        self.0
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
            .unwrap_or_else(|| write!(f, "{}", ByteSliceFormatter::Dynamic(&self.0)))
    }
}

impl fmt::Display for ErrorReplyPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Error type returned by gstd API while using invalid arguments.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UsageError {
    /// This error occurs when providing zero duration to waiting functions
    /// (e.g. see `exactly` and `up_to` functions in
    /// [`CodecMessageFuture`](crate::msg::CodecMessageFuture)).
    #[error("Wait duration can not be zero")]
    EmptyWaitDuration,
    /// This error occurs when providing zero gas amount to system gas reserving
    /// function.
    #[cfg_attr(
        not(feature = "ethexe"),
        doc = "See [`Config::set_system_reserve`](crate::Config::set_system_reserve)."
    )]
    #[cfg_attr(
        feature = "ethexe",
        doc = "Never occurs under `ethexe` feature, since `Config::set_system_reserve` method is not available."
    )]
    #[error("System reservation amount can not be zero in config")]
    ZeroSystemReservationAmount,
    /// This error occurs when providing zero duration to mutex lock function
    #[error("Mutex lock duration can not be zero")]
    ZeroMxLockDuration,
    /// This error occurs when handle_reply is called without (or with zero)
    /// reply deposit.
    #[cfg_attr(
        not(feature = "ethexe"),
        doc = "See [`MessageFuture::handle_reply`](crate::msg::MessageFuture::handle_reply)."
    )]
    #[cfg_attr(
        feature = "ethexe",
        doc = "Never occurs under `ethexe` feature, since `MessageFuture::handle_reply` method is not available."
    )]
    #[error("Reply deposit can not be zero when setting reply hook")]
    ZeroReplyDeposit,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{format, vec};

    #[test]
    fn error_unsupported_reply_display() {
        let payload = Error::UnsupportedReply(vec![1, 2, 3]);
        assert_eq!(
            format!("{payload}"),
            "Received unsupported reply '0x010203'"
        );
    }
}
