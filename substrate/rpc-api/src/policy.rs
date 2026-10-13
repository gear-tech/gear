// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Policy-related types.
//!
//! Contains a `DenyUnsafe` type that can be used to deny potentially unsafe
//! RPC when accessed externally.

use jsonrpsee::types::{error::ErrorCode, ErrorObject, ErrorObjectOwned};

/// Checks if the RPC call is safe to be called externally.
pub fn check_if_safe(ext: &jsonrpsee::Extensions) -> Result<(), UnsafeRpcError> {
    match ext
        .get::<DenyUnsafe>()
        .map(|deny_unsafe| deny_unsafe.check_if_safe())
    {
        Some(Ok(())) => Ok(()),
        Some(Err(e)) => Err(e),
        None => unreachable!("DenyUnsafe extension is always set by the substrate rpc server; qed"),
    }
}

/// Signifies whether a potentially unsafe RPC should be denied.
#[derive(Clone, Copy, Debug)]
pub enum DenyUnsafe {
    /// Denies only potentially unsafe RPCs.
    Yes,
    /// Allows calling every RPCs.
    No,
}

impl DenyUnsafe {
    /// Returns `Ok(())` if the RPCs considered unsafe are safe to call,
    /// otherwise returns `Err(UnsafeRpcError)`.
    pub fn check_if_safe(self) -> Result<(), UnsafeRpcError> {
        match self {
            DenyUnsafe::Yes => Err(UnsafeRpcError),
            DenyUnsafe::No => Ok(()),
        }
    }
}

/// Signifies whether an RPC considered unsafe is denied to be called externally.
#[derive(Debug)]
pub struct UnsafeRpcError;

impl std::fmt::Display for UnsafeRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RPC call is unsafe to be called externally")
    }
}

impl std::error::Error for UnsafeRpcError {}

impl From<UnsafeRpcError> for ErrorObjectOwned {
    fn from(e: UnsafeRpcError) -> ErrorObjectOwned {
        ErrorObject::owned(ErrorCode::MethodNotFound.code(), e.to_string(), None::<()>)
    }
}
