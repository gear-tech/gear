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

use crate::{utils, BlockProcessed, ComputeError, ProcessorExt, Result};
use ethexe_common::{
    db::{BlockMetaStorageRead, BlockMetaStorageWrite, OnChainStorageRead},
    events::{BlockEvent, RouterEvent},
    gear::GearBlock,
    SimpleBlockData,
};
use ethexe_processor::BlockProcessingResult;
use gprimitives::H256;
use std::collections::VecDeque;

pub(crate) async fn compute<
    DB: BlockMetaStorageRead + BlockMetaStorageWrite + OnChainStorageRead,
    P: ProcessorExt,
>(
    db: DB,
    mut processor: P,
    head: H256,
) -> Result<BlockProcessed> {
    for block_data in utils::collect_chain(&db, head, |meta| !meta.computed)? {
        compute_one_block(&db, &mut processor, block_data).await?;
    }
    Ok(BlockProcessed { block_hash: head })
}

async fn compute_one_block<
    DB: BlockMetaStorageRead + BlockMetaStorageWrite + OnChainStorageRead,
    P: ProcessorExt,
>(
    db: &DB,
    processor: &mut P,
    block_data: SimpleBlockData,
) -> Result<()> {
    let SimpleBlockData {
        hash: block,
        header,
    } = block_data;

    let events = db
        .block_events(block)
        .ok_or(ComputeError::BlockEventsNotFound(block))?;

    let parent = header.parent_hash;
    if !db.block_meta(parent).computed {
        unreachable!("Parent block {parent} must be computed before the current one {block}",);
    }
    let mut waiting_blocks = propagate_data_from_parent(db, block, parent, events.iter())?;

    let block_request_events = events
        .into_iter()
        .filter_map(|event| event.to_request())
        .collect();

    let processing_result = processor
        .process_block_events(block, block_request_events)
        .await?;

    let BlockProcessingResult {
        transitions,
        states,
        schedule,
    } = processing_result;

    if !transitions.is_empty() {
        waiting_blocks.push_back(block);
    }

    db.set_block_commitment_queue(block, waiting_blocks);
    db.set_block_outcome(block, transitions);
    db.set_block_program_states(block, states);
    db.set_block_schedule(block, schedule);
    db.mutate_block_meta(block, |meta| meta.computed = true);
    db.set_latest_computed_block(block, header);

    Ok(())
}

fn propagate_data_from_parent<'a, DB: BlockMetaStorageRead + BlockMetaStorageWrite>(
    db: &DB,
    block: H256,
    parent: H256,
    events: impl Iterator<Item = &'a BlockEvent>,
) -> Result<VecDeque<H256>> {
    // Propagate prev commitment (prev not empty block hash or zero for genesis).
    if db
        .block_outcome_is_empty(parent)
        .ok_or(ComputeError::ParentNotFound(block))?
    {
        let parent_prev_commitment = db
            .previous_not_empty_block(parent)
            .ok_or(ComputeError::PreviousCommitmentNotFound(parent))?;
        db.set_previous_not_empty_block(block, parent_prev_commitment);
    } else {
        db.set_previous_not_empty_block(block, parent);
    }

    let mut blocks_queue = db
        .block_commitment_queue(parent)
        .ok_or(ComputeError::CommitmentQueueNotFound(parent))?;
    for event in events {
        if let BlockEvent::Router(RouterEvent::GearBlockCommitted(GearBlock { hash, .. })) = event {
            if let Some(index) = blocks_queue
                .iter()
                .enumerate()
                .find_map(|(index, h)| (*h == *hash).then_some(index))
            {
                blocks_queue.drain(..=index);
            } else {
                log::warn!(
                    "Block {hash} not found in parent waiting blocks queue at block {parent}"
                );
            }
        }
    }

    Ok(blocks_queue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::MockProcessor;
    use ethexe_common::{
        db::{BlockMetaStorageWrite, OnChainStorageWrite},
        events::BlockEvent,
        BlockHeader,
    };
    use ethexe_db::Database as DB;
    use gprimitives::H256;
    use std::collections::VecDeque;

    /// Test compute function with chain of 3 blocks
    #[tokio::test]
    async fn test_compute() {
        let db = DB::memory();
        let processor = MockProcessor;

        // Create a chain: genesis -> block1 -> block2 -> head
        let genesis_hash = H256::from([0; 32]);
        let block1_hash = H256::from([1; 32]);
        let block2_hash = H256::from([2; 32]);
        let head_hash = H256::from([3; 32]);

        // Setup genesis block as computed
        db.mutate_block_meta(genesis_hash, |meta| meta.computed = true);
        db.set_block_commitment_queue(genesis_hash, VecDeque::new());
        db.set_block_outcome(genesis_hash, vec![]);
        db.set_previous_not_empty_block(genesis_hash, genesis_hash);
        let genesis_header = BlockHeader {
            height: 0,
            parent_hash: H256::zero(),
            timestamp: 1000,
        };
        db.set_block_header(genesis_hash, genesis_header);

        // Setup block1 as synced but not computed
        db.mutate_block_meta(block1_hash, |meta| meta.synced = true);
        let block1_header = BlockHeader {
            height: 1,
            parent_hash: genesis_hash,
            timestamp: 2000,
        };
        db.set_block_header(block1_hash, block1_header);
        db.set_block_events(block1_hash, &[]);

        // Setup block2 as synced but not computed
        db.mutate_block_meta(block2_hash, |meta| meta.synced = true);
        let block2_header = BlockHeader {
            height: 2,
            parent_hash: block1_hash,
            timestamp: 3000,
        };
        db.set_block_header(block2_hash, block2_header);
        db.set_block_events(block2_hash, &[]);

        // Setup head as synced but not computed
        db.mutate_block_meta(head_hash, |meta| meta.synced = true);
        let head_header = BlockHeader {
            height: 3,
            parent_hash: block2_hash,
            timestamp: 4000,
        };
        db.set_block_header(head_hash, head_header);
        db.set_block_events(head_hash, &[]);

        let result = compute(db.clone(), processor, head_hash).await.unwrap();

        assert_eq!(result.block_hash, head_hash);

        // Verify all blocks were computed
        assert!(db.block_meta(block1_hash).computed);
        assert!(db.block_meta(block2_hash).computed);
        assert!(db.block_meta(head_hash).computed);
    }

    /// Test compute_one_block function
    #[tokio::test]
    async fn test_compute_one_block() {
        let db = DB::memory();
        let mut processor = MockProcessor;
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);

        // Setup parent block as computed
        db.mutate_block_meta(parent_hash, |meta| meta.computed = true);
        db.set_block_commitment_queue(parent_hash, VecDeque::new());
        db.set_block_outcome(parent_hash, vec![]);
        db.set_previous_not_empty_block(parent_hash, parent_hash); // Add missing previous commitment

        // Setup block data
        let header = BlockHeader {
            height: 2,
            parent_hash,
            timestamp: 2000,
        };

        let block_data = SimpleBlockData {
            hash: block_hash,
            header,
        };

        // Setup block events
        db.set_block_events(block_hash, &[]);

        let result = compute_one_block(&db, &mut processor, block_data).await;

        assert!(result.is_ok());

        // Verify block was marked as computed
        let meta = db.block_meta(block_hash);
        assert!(meta.computed);
    }

    /// Test propagate_data_from_parent function
    #[test]
    fn test_propagate_data_from_parent() {
        let db = DB::memory();
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);
        let committed_block_hash = H256::from([3; 32]);

        // Setup parent data
        let mut parent_queue = VecDeque::new();
        parent_queue.push_back(committed_block_hash);
        parent_queue.push_back(H256::from([4; 32]));

        db.set_block_commitment_queue(parent_hash, parent_queue);
        db.set_block_outcome(parent_hash, vec![]);
        db.set_previous_not_empty_block(parent_hash, parent_hash); // Add missing previous commitment

        // Create events with GearBlockCommitted
        let events = [BlockEvent::Router(RouterEvent::GearBlockCommitted(
            GearBlock {
                hash: committed_block_hash,
                off_chain_transactions_hash: H256::zero(),
                gas_allowance: 1000,
            },
        ))];

        let result =
            propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

        // Should have one block remaining in queue (the second one)
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], H256::from([4; 32]));

        // Verify previous not empty block was set correctly
        let prev_not_empty = db.previous_not_empty_block(block_hash).unwrap();
        assert_eq!(prev_not_empty, parent_hash);
    }

    /// Test propagate_data_from_parent with empty parent outcome
    #[test]
    fn test_propagate_data_from_parent_empty_parent_outcome() {
        let db = DB::memory();
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);
        let grandparent_hash = H256::from([0; 32]);

        // Setup parent with empty outcome
        db.set_block_commitment_queue(parent_hash, VecDeque::new());
        db.set_block_outcome(parent_hash, vec![]);
        db.set_previous_not_empty_block(parent_hash, grandparent_hash);

        let events = [];

        let result =
            propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

        assert!(result.is_empty());

        // Should propagate grandparent as previous not empty block
        let prev_not_empty = db.previous_not_empty_block(block_hash).unwrap();
        assert_eq!(prev_not_empty, grandparent_hash);
    }

    /// Test compute_one_block function with non-empty processor result
    #[tokio::test]
    async fn test_compute_one_block_with_non_empty_result() {
        use crate::tests::PROCESSOR_RESULT;
        use ethexe_common::gear::StateTransition;
        use gprimitives::ActorId;
        use std::collections::BTreeMap;

        let db = DB::memory();
        let mut processor = MockProcessor;
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);

        // Setup parent block as computed
        db.mutate_block_meta(parent_hash, |meta| meta.computed = true);
        db.set_block_commitment_queue(parent_hash, VecDeque::new());
        db.set_block_outcome(parent_hash, vec![]);
        db.set_previous_not_empty_block(parent_hash, parent_hash);

        // Setup block data
        let header = BlockHeader {
            height: 2,
            parent_hash,
            timestamp: 2000,
        };

        let block_data = SimpleBlockData {
            hash: block_hash,
            header,
        };

        // Setup block events
        db.set_block_events(block_hash, &[]);

        // Create non-empty processor result with transitions
        let non_empty_result = BlockProcessingResult {
            transitions: vec![StateTransition {
                actor_id: ActorId::from([1; 32]),
                new_state_hash: H256::from([2; 32]),
                exited: false,
                inheritor: ActorId::zero(),
                value_to_receive: 100,
                value_claims: vec![],
                messages: vec![],
            }],
            states: BTreeMap::new(),
            schedule: BTreeMap::new(),
        };

        // Set the PROCESSOR_RESULT to return non-empty result
        PROCESSOR_RESULT.with(|r| *r.borrow_mut() = non_empty_result.clone());
        let result = compute_one_block(&db, &mut processor, block_data).await;

        assert!(result.is_ok());

        // Verify block was marked as computed
        let meta = db.block_meta(block_hash);
        assert!(meta.computed);

        // Verify transitions were stored in DB
        let stored_transitions = db.block_outcome(block_hash).unwrap();
        assert_eq!(stored_transitions.len(), 1);
        assert_eq!(stored_transitions[0].actor_id, ActorId::from([1; 32]));
        assert_eq!(stored_transitions[0].new_state_hash, H256::from([2; 32]));

        // Verify that block was added to waiting blocks queue since transitions are not empty
        let commitment_queue = db.block_commitment_queue(block_hash).unwrap();
        assert_eq!(commitment_queue.len(), 1);
        assert_eq!(commitment_queue[0], block_hash);
    }
}
