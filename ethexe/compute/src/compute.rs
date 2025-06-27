use crate::{utils, BlockProcessed, ComputeError, Result};
use ethexe_common::{
    db::{BlockMetaStorageRead, BlockMetaStorageWrite, OnChainStorageRead},
    events::{BlockEvent, RouterEvent},
    gear::GearBlock,
    SimpleBlockData,
};
use ethexe_processor::{BlockProcessingResult, Processor};
use gprimitives::H256;
use std::collections::VecDeque;

pub(crate) async fn compute<
    DB: BlockMetaStorageRead + BlockMetaStorageWrite + OnChainStorageRead,
>(
    db: DB,
    mut processor: Processor,
    head: H256,
) -> Result<BlockProcessed> {
    for block_data in utils::collect_chain(&db, head, |meta| !meta.computed)? {
        compute_one_block(&db, &mut processor, block_data).await?;
    }
    Ok(BlockProcessed { block_hash: head })
}

async fn compute_one_block<
    DB: BlockMetaStorageRead + BlockMetaStorageWrite + OnChainStorageRead,
>(
    db: &DB,
    processor: &mut Processor,
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
