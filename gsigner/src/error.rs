// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Error types for the gsigner library.

use alloc::string::{String, ToString};
use core::result::Result as CoreResult;
#[cfg(feature = "std")]
use std::io;

/// Result type alias using [`SignerError`].
pub type Result<T> = CoreResult<T, SignerError>;

/// Errors that can occur during signing operations.
#[derive(Debug, thiserror::Error)]
pub enum SignerError {
    /// Key not found in storage.
    #[error("Key not found: {0}")]
    KeyNotFound(String),

    /// Storage I/O error.
    #[cfg(feature = "std")]
    #[error("Storage error: {0}")]
    Storage(#[from] io::Error),

    /// Invalid key format or data.
    #[error("Invalid key: {0}")]
    InvalidKey(String),

    /// Invalid signature format or data.
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    /// Signature verification failed.
    #[error("Signature verification failed")]
    VerificationFailed,

    /// Serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Cryptographic operation failed.
    #[error("Cryptographic error: {0}")]
    Crypto(String),

    /// Invalid address format.
    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    /// Feature not enabled.
    #[error("Feature not enabled: {0}")]
    FeatureNotEnabled(&'static str),

    /// Other error.
    #[error("{0}")]
    Other(String),
}

#[cfg(feature = "std")]
impl From<serde_json::Error> for SignerError {
    fn from(err: serde_json::Error) -> Self {
        SignerError::Serialization(err.to_string())
    }
}

impl From<hex::FromHexError> for SignerError {
    fn from(err: hex::FromHexError) -> Self {
        SignerError::InvalidKey(err.to_string())
    }
}

#[cfg(feature = "secp256k1")]
impl From<k256::ecdsa::Error> for SignerError {
    fn from(err: k256::ecdsa::Error) -> Self {
        SignerError::Crypto(err.to_string())
    }
}

#[cfg(feature = "sr25519")]
impl From<schnorrkel::SignatureError> for SignerError {
    fn from(err: schnorrkel::SignatureError) -> Self {
        SignerError::Crypto(err.to_string())
    }
}
