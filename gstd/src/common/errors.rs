// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
//! Enumerates possible errors in smart contracts `Error`.
//! Errors related to conversion, decoding, message status code, other internal
//! errors.

use core::fmt;
use gcore::errors::Error as CoreError;

pub use gcore::errors::*;

/// `Result` type with a predefined error type ([`Error`]).
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Common error type returned by API functions from other modules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// [`gcore::errors::Error`] type.
    Core(CoreError),
    /// Timeout reached while expecting for reply.
    Timeout(u32, u32),
    /// Conversion error.
    Convert(&'static str),
    /// Decoding error.
    Decode(scale_info::scale::Error),
    /// Reply code returned by another program.
    ReplyCode(ReplyCode),
    /// This error occurs when providing zero duration to waiting functions
    /// (e.g. see `exactly` and `up_to` functions in
    /// [CodecMessageFuture](crate::msg::CodecMessageFuture)).
    EmptyWaitDuration,
    /// This error occurs when providing zero gas amount to system gas reserving
    /// function (see
    /// [Config::set_system_reserve](crate::Config::set_system_reserve)).
    ZeroSystemReservationAmount,
}

impl Error {
    /// Check whether an error is [`Error::Timeout`].
    pub fn timed_out(&self) -> bool {
        matches!(self, Error::Timeout(..))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Core(e) => fmt::Display::fmt(e, f),
            Error::Timeout(expected, now) => {
                write!(f, "Wait lock timeout at {expected}, now is {now}")
            }
            Error::Convert(e) => write!(f, "Conversion error: {e:?}"),
            Error::Decode(e) => write!(f, "Decoding codec bytes error: {e}"),
            Error::ReplyCode(e) => write!(f, "Reply came with non success reply code {e:?}"),
            Error::EmptyWaitDuration => write!(f, "Wait duration can not be zero."),
            Error::ZeroSystemReservationAmount => {
                write!(f, "System reservation amount can not be zero in config.")
            }
        }
    }
}

impl From<CoreError> for Error {
    fn from(err: CoreError) -> Self {
        Self::Core(err)
    }
}

pub(crate) trait IntoResult<T> {
    fn into_result(self) -> Result<T>;
}

impl<T, E, V> IntoResult<V> for core::result::Result<T, E>
where
    T: Into<V>,
    E: Into<Error>,
{
    fn into_result(self) -> Result<V> {
        self.map(Into::into).map_err(Into::into)
    }
}
