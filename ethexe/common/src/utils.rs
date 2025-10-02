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
    Address, Announce, AnnounceHash, SimpleBlockData,
    db::{
        AnnounceStorageRead, AnnounceStorageWrite, BlockMeta, BlockMetaStorageWrite,
        FullAnnounceData, FullBlockData, LatestData, LatestDataStorageRead, LatestDataStorageWrite,
        OnChainStorageRead, OnChainStorageWrite,
    },
};
use alloc::vec::Vec;
use gprimitives::H256;
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

pub fn setup_start_block_in_db<
    DB: OnChainStorageWrite + BlockMetaStorageWrite + AnnounceStorageWrite + LatestDataStorageWrite,
>(
    db: &DB,
    start_block_hash: H256,
    start_block_data: FullBlockData,
    start_announce_data: FullAnnounceData,
) {
    let height = start_block_data.header.height;
    let announce_hash = start_announce_data.announce.to_hash();

    assert_eq!(
        start_block_data.announces,
        [announce_hash].into(),
        "start block and announce data incompatible"
    );

    setup_block_in_db(db, start_block_hash, start_block_data);
    setup_announce_in_db(db, start_announce_data);

    db.mutate_latest_data(|latest| {
        latest.synced_block_height = height;
        latest.prepared_block_hash = start_block_hash;
        latest.computed_announce_hash = announce_hash;
        latest.start_block_hash = start_block_hash;
        latest.start_announce_hash = announce_hash;
    })
    .expect("Latest data must be set before `setup_genesis_in_db` calling");
}

pub fn setup_genesis_in_db<
    DB: OnChainStorageWrite + BlockMetaStorageWrite + AnnounceStorageWrite + LatestDataStorageWrite,
>(
    db: &DB,
    genesis_block: SimpleBlockData,
    validators: NonEmpty<Address>,
) {
    let genesis_announce = Announce::base(genesis_block.hash, AnnounceHash::zero());
    let genesis_announce_hash = setup_announce_in_db(
        db,
        FullAnnounceData {
            announce: genesis_announce,
            program_states: Default::default(),
            outcome: Default::default(),
            schedule: Default::default(),
        },
    );

    setup_block_in_db(
        db,
        genesis_block.hash,
        FullBlockData {
            header: genesis_block.header,
            events: Default::default(),
            validators: validators.clone(),

            codes_queue: Default::default(),
            announces: [genesis_announce_hash].into(),
            last_committed_batch: Default::default(),
            last_committed_announce: Default::default(),
        },
    );

    if let Some(latest) = db.latest_data() {
        assert_eq!(
            latest.genesis_block_hash, genesis_block.hash,
            "genesis_block_hash mismatch - you should clean database"
        );
        assert_eq!(
            latest.genesis_announce_hash, genesis_announce_hash,
            "genesis_announce_hash mismatch - you should clean database"
        );
    } else {
        db.set_latest_data(LatestData {
            synced_block_height: genesis_block.header.height,
            prepared_block_hash: genesis_block.hash,
            computed_announce_hash: genesis_announce_hash,
            genesis_block_hash: genesis_block.hash,
            genesis_announce_hash,
            start_block_hash: genesis_block.hash,
            start_announce_hash: genesis_announce_hash,
        });
    }
}

pub fn setup_block_in_db<DB: OnChainStorageWrite + BlockMetaStorageWrite>(
    db: &DB,
    block_hash: H256,
    block_data: FullBlockData,
) {
    db.set_block_header(block_hash, block_data.header);
    db.set_block_events(block_hash, &block_data.events);
    db.set_block_validators(block_hash, block_data.validators);
    db.set_block_synced(block_hash);

    db.mutate_block_meta(block_hash, |meta| {
        *meta = BlockMeta {
            prepared: true,
            announces: Some(block_data.announces),
            codes_queue: Some(block_data.codes_queue),
            last_committed_batch: Some(block_data.last_committed_batch),
            last_committed_announce: Some(block_data.last_committed_announce),
        }
    });
}

pub fn setup_announce_in_db<DB: AnnounceStorageWrite>(
    db: &DB,
    announce_data: FullAnnounceData,
) -> AnnounceHash {
    let announce_hash = announce_data.announce.to_hash();
    db.set_announce(announce_data.announce);
    db.set_announce_program_states(announce_hash, announce_data.program_states);
    db.set_announce_outcome(announce_hash, announce_data.outcome);
    db.set_announce_schedule(announce_hash, announce_data.schedule);
    db.mutate_announce_meta(announce_hash, |meta| meta.computed = true);

    announce_hash
}

pub fn announce_is_successor_of<
    DB: AnnounceStorageRead + OnChainStorageRead + LatestDataStorageRead,
>(
    db: &DB,
    announce_hash: AnnounceHash,
    potential_predecessor_hash: AnnounceHash,
) -> Result<bool, anyhow::Error> {
    let predecessor_announce_block_height = if potential_predecessor_hash != AnnounceHash::zero() {
        db.block_header(
            db.announce(potential_predecessor_hash)
                .ok_or_else(|| {
                    anyhow::anyhow!("No announce found for {potential_predecessor_hash} in db")
                })?
                .block_hash,
        )
        .ok_or_else(|| anyhow::anyhow!("No block header found for announce block"))?
        .height
    } else {
        let genesis = db
            .latest_data()
            .ok_or_else(|| anyhow::anyhow!("No latest data found in db"))?
            .genesis_block_hash;
        db.block_header(genesis)
            .ok_or_else(|| anyhow::anyhow!("No block header found for genesis block"))?
            .height
            - 1
    };

    let announce_block_height = db
        .block_header(
            db.announce(announce_hash)
                .ok_or_else(|| anyhow::anyhow!("No announce found for {announce_hash} in db"))?
                .block_hash,
        )
        .ok_or_else(|| anyhow::anyhow!("No block header found for announce block"))?
        .height;

    if announce_block_height < predecessor_announce_block_height {
        return Ok(false);
    }

    let mut current_hash = announce_hash;
    for _ in predecessor_announce_block_height..=announce_block_height {
        if current_hash == potential_predecessor_hash {
            return Ok(true);
        }

        let announce = db
            .announce(current_hash)
            .ok_or_else(|| anyhow::anyhow!("No announce found for {current_hash} in db"))?;

        current_hash = announce.parent;
    }

    Ok(false)
}

pub fn announces_chain(
    db: &impl AnnounceStorageRead,
    mut head: AnnounceHash,
    tail: Option<AnnounceHash>,
) -> Result<Vec<Announce>, anyhow::Error> {
    let mut result = Vec::new();
    loop {
        if head == tail.unwrap_or(AnnounceHash::zero()) {
            break;
        }

        let announce = db
            .announce(head)
            .ok_or_else(|| anyhow::anyhow!("No announce found for {head} in db"))?;
        let parent = announce.parent;
        result.push(announce);

        head = parent;
    }

    Ok(result)
}
