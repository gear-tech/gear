// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! gear api utils
use crate::{
    Api, AsGear, Event, Result, TxInBlock,
    config::GearConfig,
    gear::{DispatchError, system},
};
use parity_scale_codec::Encode;
use sp_core::hashing;
use subxt::{
    Metadata, OnlineClient,
    blocks::ExtrinsicEvents,
    error::DispatchError as SubxtDispatchError,
    storage::{Address, Storage},
    utils::H256,
};

impl Api {
    /// Decode `DispatchError` to `subxt::error::Error`.
    pub fn decode_error(&self, dispatch_error: DispatchError) -> subxt::Error {
        match SubxtDispatchError::decode_from(dispatch_error.encode(), self.metadata()) {
            Ok(err) => err.into(),
            Err(err) => err,
        }
    }

    /// Get storage from optional block hash.
    pub async fn storage_at(
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

    /// Capture the dispatch info of any extrinsic and display the weight spent
    pub async fn capture_dispatch_info(
        &self,
        tx: &TxInBlock,
    ) -> Result<ExtrinsicEvents<GearConfig>> {
        let events = tx.fetch_events().await?;

        for ev in events.iter() {
            if let Event::System(system_event) = ev?.as_gear()? {
                let extrinsic_result = match system_event {
                    system::Event::ExtrinsicFailed {
                        dispatch_error,
                        dispatch_info,
                    } => Some((dispatch_info, Err(self.decode_error(dispatch_error)))),
                    system::Event::ExtrinsicSuccess { dispatch_info } => {
                        Some((dispatch_info, Ok(())))
                    }
                    _ => None,
                };

                if let Some((dispatch_info, result)) = extrinsic_result {
                    log::info!("	Weight cost: {:?}", dispatch_info.weight);
                    result?;
                    break;
                }
            }
        }

        Ok(events)
    }
}

/// Return the root of a given [`StorageAddress`]: hash the pallet name and entry name
/// and append those bytes to the output.
pub(crate) fn write_storage_address_root_bytes(addr: &impl Address, out: &mut Vec<u8>) {
    out.extend(hashing::twox_128(addr.pallet_name().as_bytes()));
    out.extend(hashing::twox_128(addr.entry_name().as_bytes()));
}

/// Outputs the [`storage_address_root_bytes`] as well as any additional bytes that represent
/// a lookup in a storage map at that location.
pub(crate) fn storage_address_bytes(
    addr: &impl Address,
    metadata: &Metadata,
) -> Result<Vec<u8>, Box<subxt::Error>> {
    let mut bytes = Vec::new();
    write_storage_address_root_bytes(addr, &mut bytes);
    addr.append_entry_bytes(metadata, &mut bytes)
        .map_err(|e| Box::new(e.into()))?;
    Ok(bytes)
}

/// Convert hex string to byte array.
pub(crate) fn hex_to_vec(string: impl AsRef<str>) -> Result<Vec<u8>> {
    hex::decode(string.as_ref().trim_start_matches("0x")).map_err(Into::into)
}
