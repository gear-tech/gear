// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Offchain RPC errors.

use jsonrpsee::types::error::{ErrorObject, ErrorObjectOwned};

/// Offchain RPC Result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Offchain RPC errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Unavailable storage kind error.
    #[error("This storage kind is not available yet.")]
    UnavailableStorageKind,
    /// Call to an unsafe RPC was denied.
    #[error(transparent)]
    UnsafeRpcCalled(#[from] crate::policy::UnsafeRpcError),
}

/// Base error code for all offchain errors.
const BASE_ERROR: i32 = crate::error::base::OFFCHAIN;

impl From<Error> for ErrorObjectOwned {
    fn from(e: Error) -> Self {
        match e {
            Error::UnavailableStorageKind => ErrorObject::owned(
                BASE_ERROR + 1,
                "This storage kind is not available yet",
                None::<()>,
            ),
            Error::UnsafeRpcCalled(e) => e.into(),
        }
    }
}
