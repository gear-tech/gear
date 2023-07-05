// This file is part of Gear.
//
// Copyright (C) 2022-2023 Gear Technologies Inc.
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

//! Type definitions and helpers for error handling.

use core::fmt;

pub use gear_core_errors::*;

/// `Result` type with a predefined error type ([`ExtError`]).
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Common error type returned by API functions from other modules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// Syscall usage error.
    SyscallUsage,
    /// API error (see [`ExtError`] for details).
    Ext(ExtError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::SyscallUsage => write!(f, "syscall usage error"),
            Error::Ext(e) => write!(f, "{}", e),
        }
    }
}

impl From<ExtError> for Error {
    fn from(err: ExtError) -> Self {
        Error::Ext(err)
    }
}

/// Syscall executing result.
///
/// The wrapped value is the length of the error string, if any.
#[must_use]
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SyscallError(pub(crate) u32);

impl From<SyscallError> for Result<()> {
    fn from(value: SyscallError) -> Self {
        match value.0 {
            0 => Ok(()),
            code => {
                Err(ExtError::from_u32(code).unwrap_or(ExtError::Unsupported)).map_err(Into::into)
            }
        }
    }
}

impl SyscallError {
    /// Convert `SyscallError` into `Result`.
    pub fn into_result(self) -> Result<()> {
        self.into()
    }
}
