// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::{
    Address, Announce, AnnounceHash, Digest, SimpleBlockData,
    db::{
        AnnounceStorageWrite, BlockMeta, BlockMetaStorageWrite, LatestData, LatestDataStorageWrite,
        OnChainStorageWrite,
    },
};
use alloc::vec;
use nonempty::NonEmpty;

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

pub fn setup_genesis_in_db<
    DB: OnChainStorageWrite + BlockMetaStorageWrite + AnnounceStorageWrite + LatestDataStorageWrite,
>(
    db: &DB,
    genesis_block: SimpleBlockData,
    validators: NonEmpty<Address>,
) {
    db.set_block_header(genesis_block.hash, genesis_block.header);
    db.set_block_events(genesis_block.hash, &[]);
    db.set_validators(genesis_block.hash, validators);
    db.set_block_synced(genesis_block.hash);

    let genesis_announce = Announce::base(genesis_block.hash, AnnounceHash::zero());
    let genesis_announce_hash = db.set_announce(genesis_announce);
    db.set_announce_outcome(genesis_announce_hash, vec![]);
    db.set_announce_program_states(genesis_announce_hash, Default::default());
    db.set_announce_schedule(genesis_announce_hash, Default::default());
    db.mutate_announce_meta(genesis_announce_hash, |meta| meta.computed = true);

    // Genesis block is the only one block where announce is committed in the same block.
    db.mutate_block_meta(genesis_block.hash, |meta| {
        *meta = BlockMeta {
            prepared: true,
            announces: Some(vec![genesis_announce_hash]),
            codes_queue: Some(Default::default()),
            last_committed_batch: Some(Digest::zero()),
            last_committed_announce: Some(genesis_announce_hash),
        }
    });

    db.mutate_latest_data(|data| {
        data.get_or_insert(LatestData {
            synced_block_height: genesis_block.header.height,
            prepared_block_hash: genesis_block.hash,
            computed_announce_hash: genesis_announce_hash,
            genesis_block_hash: genesis_block.hash,
            genesis_announce_hash,
            start_block_hash: genesis_block.hash,
            start_announce_hash: genesis_announce_hash,
        });
    });
}
