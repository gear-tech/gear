// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! System RPC module errors.

use crate::system::helpers::Health;
use jsonrpsee::types::{
    error::{ErrorCode, ErrorObject},
    ErrorObjectOwned,
};

/// System RPC Result type.
pub type Result<T> = std::result::Result<T, Error>;

/// System RPC errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Provided block range couldn't be resolved to a list of blocks.
    #[error("Node is not fully functional: {}", .0)]
    NotHealthy(Health),
    /// Peer argument is malformatted.
    #[error("{0}")]
    MalformattedPeerArg(String),
    /// Call to an unsafe RPC was denied.
    #[error(transparent)]
    UnsafeRpcCalled(#[from] crate::policy::UnsafeRpcError),
    /// Internal error.
    #[error("{0}")]
    Internal(String),
}

// Base code for all system errors.
const BASE_ERROR: i32 = crate::error::base::SYSTEM;
// Provided block range couldn't be resolved to a list of blocks.
const NOT_HEALTHY_ERROR: i32 = BASE_ERROR + 1;
// Peer argument is malformatted.
const MALFORMATTED_PEER_ARG_ERROR: i32 = BASE_ERROR + 2;

impl From<Error> for ErrorObjectOwned {
    fn from(e: Error) -> ErrorObjectOwned {
        match e {
            Error::NotHealthy(ref h) => {
                ErrorObject::owned(NOT_HEALTHY_ERROR, e.to_string(), Some(h))
            }
            Error::MalformattedPeerArg(e) => {
                ErrorObject::owned(MALFORMATTED_PEER_ARG_ERROR, e, None::<()>)
            }
            Error::UnsafeRpcCalled(e) => e.into(),
            Error::Internal(e) => {
                ErrorObjectOwned::owned(ErrorCode::InternalError.code(), e, None::<()>)
            }
        }
    }
}
