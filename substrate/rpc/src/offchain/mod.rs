// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Substrate offchain API.

#[cfg(test)]
mod tests;

use self::error::Error;
use jsonrpsee::{core::async_trait, Extensions};
use parking_lot::RwLock;
use sc_rpc_api::check_if_safe;
/// Re-export the API for backward compatibility.
pub use sc_rpc_api::offchain::*;
use sp_core::{
    offchain::{OffchainStorage, StorageKind},
    Bytes,
};
use std::sync::Arc;

/// Offchain API
#[derive(Debug)]
pub struct Offchain<T: OffchainStorage> {
    /// Offchain storage
    storage: Arc<RwLock<T>>,
}

impl<T: OffchainStorage> Offchain<T> {
    /// Create new instance of Offchain API.
    pub fn new(storage: T) -> Self {
        Offchain {
            storage: Arc::new(RwLock::new(storage)),
        }
    }
}

#[async_trait]
impl<T: OffchainStorage + 'static> OffchainApiServer for Offchain<T> {
    fn set_local_storage(
        &self,
        ext: &Extensions,
        kind: StorageKind,
        key: Bytes,
        value: Bytes,
    ) -> Result<(), Error> {
        check_if_safe(ext)?;

        let prefix = match kind {
            StorageKind::PERSISTENT => sp_offchain::STORAGE_PREFIX,
            StorageKind::LOCAL => return Err(Error::UnavailableStorageKind),
        };
        self.storage.write().set(prefix, &key, &value);
        Ok(())
    }

    fn get_local_storage(
        &self,
        ext: &Extensions,
        kind: StorageKind,
        key: Bytes,
    ) -> Result<Option<Bytes>, Error> {
        check_if_safe(ext)?;

        let prefix = match kind {
            StorageKind::PERSISTENT => sp_offchain::STORAGE_PREFIX,
            StorageKind::LOCAL => return Err(Error::UnavailableStorageKind),
        };

        Ok(self.storage.read().get(prefix, &key).map(Into::into))
    }
}
