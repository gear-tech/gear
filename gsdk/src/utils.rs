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
    metadata::{DispatchError, StorageInfo},
    result::Result,
    Api,
};
use parity_scale_codec::Encode;
use sp_core::H256;
use subxt::{
    dynamic::Value,
    error::{DispatchError as SubxtDispatchError, Error},
    storage::{DynamicAddress, Storage, StorageKey},
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
    pub fn storage<T: StorageInfo, Keys: StorageKey>(
        storage: T,
        keys: Keys,
    ) -> DynamicAddress<Keys> {
        subxt::dynamic::storage(T::PALLET, storage.storage_name(), keys)
    }

    /// Get the storage root address from storage info.
    pub fn storage_root<T: StorageInfo>(storage: T) -> DynamicAddress<Vec<Value>> {
        subxt::dynamic::storage(T::PALLET, storage.storage_name(), Default::default())
    }
}
