// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Statement RPC errors.

use jsonrpsee::types::error::{ErrorObject, ErrorObjectOwned};

/// Statement RPC Result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Statement RPC errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Statement store internal error.
    #[error("Statement store error")]
    StatementStore(String),
    /// Call to an unsafe RPC was denied.
    #[error(transparent)]
    UnsafeRpcCalled(#[from] crate::policy::UnsafeRpcError),
}

/// Base error code for all statement errors.
const BASE_ERROR: i32 = crate::error::base::STATEMENT;

impl From<Error> for ErrorObjectOwned {
    fn from(e: Error) -> Self {
        match e {
            Error::StatementStore(message) => ErrorObject::owned(
                BASE_ERROR + 1,
                format!("Statement store error: {message}"),
                None::<()>,
            ),
            Error::UnsafeRpcCalled(e) => e.into(),
        }
    }
}
