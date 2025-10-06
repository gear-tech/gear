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

use crate::{ComputeError, Result};
use ethexe_common::{
    Announce, AnnounceHash, SimpleBlockData,
    db::{
        AnnounceStorageRead, AnnounceStorageWrite, BlockMeta, BlockMetaStorageRead,
        BlockMetaStorageWrite, OnChainStorageRead,
    },
};
use gprimitives::H256;
use std::collections::VecDeque;

/// Collect a chain of blocks from the head to the last block that satisfies the filter.
/// Stops when the filter returns false for the block meta.
/// Returns a chain sorted in order from the oldest to the newest block (head is newest).
pub fn collect_chain<DB: BlockMetaStorageRead + OnChainStorageRead>(
    db: &DB,
    head: H256,
    mut filter: impl FnMut(&BlockMeta) -> bool,
) -> Result<VecDeque<SimpleBlockData>> {
    let mut block = head;
    let mut chain = VecDeque::new();

    while filter(&db.block_meta(block)) {
        let header = db
            .block_header(block)
            .ok_or(ComputeError::BlockHeaderNotFound(block))?;

        let parent = header.parent_hash;

        chain.push_front(SimpleBlockData {
            hash: block,
            header,
        });

        block = parent;
    }

    Ok(chain)
}

/// Announce included - means announce is part of the block.
/// Compute service guarantees that for included announce all its predecessors are included
pub fn announce_is_included<DB: BlockMetaStorageRead + AnnounceStorageRead>(
    db: &DB,
    announce_hash: AnnounceHash,
) -> bool {
    db.announce(announce_hash)
        .and_then(|announce| db.block_meta(announce.block_hash).announces)
        .into_iter()
        .flat_map(|x| x.into_iter())
        .any(|hash| hash == announce_hash)
}

pub fn not_computed_chain<DB: AnnounceStorageRead>(
    db: &DB,
    mut announce_hash: AnnounceHash,
) -> Result<VecDeque<Announce>> {
    let mut not_computed_chain = VecDeque::new();
    while !db.announce_meta(announce_hash).computed {
        let announce = db
            .announce(announce_hash)
            .ok_or(ComputeError::AnnounceNotFound(announce_hash))?;
        announce_hash = announce.parent;
        not_computed_chain.push_front(announce);
    }
    Ok(not_computed_chain)
}

pub fn include_one<DB: BlockMetaStorageWrite + AnnounceStorageWrite>(
    db: &DB,
    announce: Announce,
) -> Result<AnnounceHash> {
    let block_hash = announce.block_hash;
    let announce_hash = announce.to_hash();

    let mut announces = db
        .block_meta(block_hash)
        .announces
        .ok_or(ComputeError::PreparedBlockAnnouncesSetMissing(block_hash))?;

    if announces.iter().any(|&h| h == announce_hash) {
        log::error!("{announce_hash} is already included in block {block_hash}");
        return Ok(announce_hash);
    }

    if !announce.is_base() {
        // Check whether we have already announces from producer for this block
        for &hash in announces.iter() {
            let neighbor_announce = db
                .announce(hash)
                .ok_or(ComputeError::AnnounceNotFound(hash))?;
            if !neighbor_announce.is_base() {
                // TODO +_+_+: decide what to do in that case, currently we include both
                log::warn!(
                    "Double announcement detected!!! old {}, new {announce_hash}",
                    neighbor_announce.to_hash()
                );
                break;
            }
        }
    }

    announces.insert(announce_hash);

    db.set_announce(announce);

    db.mutate_block_meta(block_hash, |meta| {
        meta.announces = Some(announces);
    });

    Ok(announce_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{BlockHeader, db::*};
    use ethexe_db::Database as DB;
    use gprimitives::H256;

    /// Test collect_chain function
    #[test]
    fn test_collect_chain() {
        let db = DB::memory();

        // Create a chain of blocks: genesis -> block1 -> block2 -> head
        let genesis_hash = H256::from([0; 32]);
        let block1_hash = H256::from([1; 32]);
        let block2_hash = H256::from([2; 32]);
        let head_hash = H256::from([3; 32]);

        // Setup genesis block (prepared)
        db.mutate_block_meta(genesis_hash, |meta| {
            meta.prepared = true;
        });
        let genesis_header = BlockHeader {
            height: 0,
            parent_hash: H256::zero(),
            timestamp: 1000,
        };
        db.set_block_header(genesis_hash, genesis_header);

        // Setup block1 (not prepared)
        db.mutate_block_meta(block1_hash, |meta| {
            meta.prepared = false;
        });
        let block1_header = BlockHeader {
            height: 1,
            parent_hash: genesis_hash,
            timestamp: 2000,
        };
        db.set_block_header(block1_hash, block1_header);

        // Setup block2 (not prepared)
        db.mutate_block_meta(block2_hash, |meta| {
            meta.prepared = false;
        });
        let block2_header = BlockHeader {
            height: 2,
            parent_hash: block1_hash,
            timestamp: 3000,
        };
        db.set_block_header(block2_hash, block2_header);

        // Setup head (not prepared)
        db.mutate_block_meta(head_hash, |meta| {
            meta.prepared = false;
        });
        let head_header = BlockHeader {
            height: 3,
            parent_hash: block2_hash,
            timestamp: 4000,
        };
        db.set_block_header(head_hash, head_header);

        // Test: collect all unprepared blocks
        let result = collect_chain(&db, head_hash, |meta| !meta.prepared).unwrap();

        // Should return chain from oldest to newest: block1 -> block2 -> head
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].hash, block1_hash);
        assert_eq!(result[0].header, block1_header);
        assert_eq!(result[1].hash, block2_hash);
        assert_eq!(result[1].header, block2_header);
        assert_eq!(result[2].hash, head_hash);
        assert_eq!(result[2].header, head_header);

        // Test: collect with filter that stops at block2
        let result = collect_chain(&db, head_hash, |meta| !meta.prepared).unwrap();

        // Should return the same result since all blocks match the filter
        assert_eq!(result.len(), 3);

        // Test: collect with filter that accepts nothing
        let result = collect_chain(&db, head_hash, |_meta| false).unwrap();

        // Should return empty chain
        assert!(result.is_empty());
    }

    /// Test collect_chain with missing header
    #[test]
    fn test_collect_chain_missing_header() {
        let db = DB::memory();
        let head_hash = H256::from([1; 32]);

        // Setup block meta but no header
        db.mutate_block_meta(head_hash, |meta| {
            meta.prepared = false;
        });

        // Should return BlockHeaderNotFound error
        let result = collect_chain(&db, head_hash, |meta| !meta.prepared);

        assert!(matches!(
            result,
            Err(ComputeError::BlockHeaderNotFound(hash)) if hash == head_hash
        ));
    }
}
