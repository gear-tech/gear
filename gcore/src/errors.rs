// This file is part of Gear.
//
// Copyright (C) 2022 Gear Technologies Inc.
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

pub use gear_core_errors::*;

/// `Result` type with a predefined error type ([`ExtError`]).
pub type Result<T, E = ExtError> = core::result::Result<T, E>;

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
            _ => Err(value.get_err()?),
        }
    }
}

impl SyscallError {
    /// Convert `SyscallError` into `Result`.
    pub fn into_result(self) -> Result<()> {
        self.into()
    }

    // TODO: issue #1859
    // We get an error using the `gr_error` syscall, which expects the error
    // occurred earlier in another syscall, or you'll get a trap. We believe error
    // decoding is successful because we use the SCALE codec crate of identical
    // versions (at least major ones) to encode and decode errors, so error
    // representation stays the same. You'll get a trap if the `len` argument is
    // less than the actual encoded error length.
    #[cfg(feature = "codec")]
    fn get_err(self) -> Result<ExtError> {
        use alloc::vec;
        use codec::Decode;

        let mut error = vec![0; self.0 as usize];
        let mut len = 0u32;

        // We hope `gr_error` returns `Ok`; otherwise, we fall into recursion.
        unsafe { gsys::gr_error(error.as_mut_ptr(), &mut len as *mut u32) };

        Self(len).into_result()?;

        Ok(ExtError::decode(&mut error.as_ref())
            .unwrap_or_else(|e| unreachable!("Failed to decode error: {}", e)))
    }

    #[cfg(not(feature = "codec"))]
    fn get_err(self) -> Result<ExtError> {
        Ok(ExtError::Some)
    }
}
