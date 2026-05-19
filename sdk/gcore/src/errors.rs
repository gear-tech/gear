// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Type definitions and helpers for error handling.

pub use gear_core_errors::*;
pub use gprimitives::ConversionError;

/// `Result` type with a predefined error type ([`ExtError`]).
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Common error type returned by API functions from other modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
pub enum Error {
    /// Syscall usage error.
    #[error("syscall usage error")]
    SyscallUsage,
    /// API error (see [`ExtError`] for details).
    #[error(transparent)]
    Ext(#[from] ExtError),
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
            code => Err(ExtError::from_u32(code)
                .unwrap_or(ExtError::Unsupported)
                .into()),
        }
    }
}

impl SyscallError {
    /// Convert `SyscallError` into `Result`.
    pub fn into_result(self) -> Result<()> {
        self.into()
    }
}
