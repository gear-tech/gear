// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Substrate offchain API.

pub mod error;

use error::Error;
use jsonrpsee::proc_macros::rpc;
use sp_core::{offchain::StorageKind, Bytes};

/// Substrate offchain RPC API
#[rpc(client, server)]
pub trait OffchainApi {
    /// Set offchain local storage under given key and prefix.
    #[method(name = "offchain_localStorageSet", with_extensions)]
    fn set_local_storage(&self, kind: StorageKind, key: Bytes, value: Bytes) -> Result<(), Error>;

    /// Get offchain local storage under given key and prefix.
    #[method(name = "offchain_localStorageGet", with_extensions)]
    fn get_local_storage(&self, kind: StorageKind, key: Bytes) -> Result<Option<Bytes>, Error>;
}
