// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! gear api utils
use crate::{
    config::GearConfig,
    ext::sp_core::hashing,
    metadata::{DispatchError, StorageInfo},
    result::Result,
    Api,
};
use parity_scale_codec::Encode;
use sp_core::H256;
use subxt::{
    dynamic::Value,
    error::{DispatchError as SubxtDispatchError, Error},
    metadata::{EncodeWithMetadata, Metadata},
    storage::{DynamicAddress, Storage, StorageAddress},
    OnlineClient,
};

impl Api {
    /// compare gas limit
    pub fn cmp_gas_limit(&self, gas: u64) -> Result<u64> {
        if let Ok(limit) = self.gas_limit() {
            Ok(if gas > limit {
                log::warn!("gas limit too high, use {} from the chain config", limit);
                limit
            } else {
                gas
            })
        } else {
            Ok(gas)
        }
    }

    /// Decode `DispatchError` to `subxt::error::Error`.
    pub fn decode_error(&self, dispatch_error: DispatchError) -> Error {
        match SubxtDispatchError::decode_from(dispatch_error.encode(), self.metadata()) {
            Ok(err) => err.into(),
            Err(err) => err,
        }
    }

    /// Get storage from optional block hash.
    pub async fn get_storage(
        &self,
        block_hash: Option<H256>,
    ) -> Result<Storage<GearConfig, OnlineClient<GearConfig>>> {
        let client = self.storage();
        let storage = if let Some(h) = block_hash {
            client.at(h)
        } else {
            client.at_latest().await?
        };

        Ok(storage)
    }

    /// Get the storage address from storage info.
    pub fn storage<Encodable: EncodeWithMetadata, T: StorageInfo>(
        storage: T,
        encodable: Vec<Encodable>,
    ) -> DynamicAddress<Encodable> {
        subxt::dynamic::storage(T::PALLET, storage.storage_name(), encodable)
    }

    /// Get the storage root address from storage info.
    pub fn storage_root<T: StorageInfo>(storage: T) -> DynamicAddress<Value> {
        subxt::dynamic::storage(T::PALLET, storage.storage_name(), vec![])
    }
}

/// Return the root of a given [`StorageAddress`]: hash the pallet name and entry name
/// and append those bytes to the output.
pub(crate) fn write_storage_address_root_bytes<Address: StorageAddress>(
    addr: &Address,
    out: &mut Vec<u8>,
) {
    out.extend(hashing::twox_128(addr.pallet_name().as_bytes()));
    out.extend(hashing::twox_128(addr.entry_name().as_bytes()));
}

/// Outputs the [`storage_address_root_bytes`] as well as any additional bytes that represent
/// a lookup in a storage map at that location.
pub(crate) fn storage_address_bytes<Address: StorageAddress>(
    addr: &Address,
    metadata: &Metadata,
) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    write_storage_address_root_bytes(addr, &mut bytes);
    addr.append_entry_bytes(metadata, &mut bytes)?;
    Ok(bytes)
}

/// Interface for adapting optional values
pub trait AsOption<T: ?Sized> {
    fn as_option(&self) -> Option<&T>;
}

impl AsOption<str> for &str {
    fn as_option(&self) -> Option<&str> {
        Some(self)
    }
}

impl AsOption<str> for Option<&str> {
    fn as_option(&self) -> Option<&str> {
        self.as_deref()
    }
}

impl AsOption<str> for &String {
    fn as_option(&self) -> Option<&str> {
        Some(self.as_ref())
    }
}

impl AsOption<str> for String {
    fn as_option(&self) -> Option<&str> {
        Some(self)
    }
}

impl AsOption<u64> for u64 {
    fn as_option(&self) -> Option<&u64> {
        Some(self)
    }
}

impl AsOption<u64> for Option<u64> {
    fn as_option(&self) -> Option<&u64> {
        self.as_ref()
    }
}
