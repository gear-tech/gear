// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::db::{BlockMeta, BlockMetaStorageRW, OnChainStorageRW, PreparedBlockData};
use gprimitives::H256;

/// Decodes hexed string to a byte array.
pub fn decode_to_array<const N: usize>(s: &str) -> Result<[u8; N], hex::FromHexError> {
    // Strip the "0x" prefix if it exists.
    let stripped = s.strip_prefix("0x").unwrap_or(s);

    // Decode
    let mut buf = [0u8; N];
    hex::decode_to_slice(stripped, &mut buf)?;

    Ok(buf)
}

/// Converts u64 to a 48-bit unsigned integer, represented as a byte array in big-endian order.
pub const fn u64_into_uint48_be_bytes_lossy(val: u64) -> [u8; 6] {
    let [_, _, b1, b2, b3, b4, b5, b6] = val.to_be_bytes();

    [b1, b2, b3, b4, b5, b6]
}

pub fn setup_block_in_db<DB: OnChainStorageRW + BlockMetaStorageRW>(
    db: &DB,
    block_hash: H256,
    block_data: PreparedBlockData,
) {
    db.set_block_header(block_hash, block_data.header);
    db.set_block_events(block_hash, &block_data.events);
    db.set_block_synced(block_hash);

    db.mutate_block_meta(block_hash, |meta| {
        *meta = BlockMeta {
            prepared: true,
            codes_queue: Some(block_data.codes_queue),
            last_committed_batch: Some(block_data.last_committed_batch),
            last_committed_mb: Some(block_data.last_committed_mb),
            last_committed_eb: Some(block_data.last_committed_eb),
            latest_era_validators_committed: Some(block_data.latest_era_with_committed_validators),
        }
    });
}
