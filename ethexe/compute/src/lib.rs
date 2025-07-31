// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Sequencer for ethexe.

use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{BlockMetaStorage, OnChainStorage},
    events::{BlockEvent, RouterEvent},
    gear::CodeCommitment,
    SimpleBlockData,
};
use ethexe_db::Database;
use ethexe_processor::{LocalOutcome, Processor};
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream};
use gprimitives::{CodeId, H256};
use std::{
    collections::{BTreeSet, VecDeque},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub struct BlockProcessed {
    pub block_hash: H256,
}

#[derive(Debug, Clone)]
pub enum ComputeEvent {
    BlockProcessed(BlockProcessed),
    CodeProcessed(CodeCommitment),
}

// TODO #4548: add state monitoring in prometheus
// TODO #4549: add tests for compute service
pub struct ComputeService {
    db: Database,
    processor: Processor,
    blocks_queue: VecDeque<H256>,
    process_block: Option<BoxFuture<'static, Result<BlockProcessed>>>,
    process_codes: JoinSet<Result<CodeCommitment>>,
}

impl Stream for ComputeService {
    type Item = Result<ComputeEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(Poll::Ready(res)) = self.process_block.as_mut().map(|f| f.poll_unpin(cx)) {
            self.process_block = self.blocks_queue.pop_front().map(|block| {
                ChainHeadProcessContext {
                    db: self.db.clone(),
                    processor: self.processor.clone(),
                }
                .process(block)
                .boxed()
            });

            return Poll::Ready(Some(res.map(ComputeEvent::BlockProcessed)));
        }

        if let Poll::Ready(Some(res)) = self.process_codes.poll_join_next(cx) {
            return Poll::Ready(Some(
                res.map_err(Into::into)
                    .and_then(|res| res.map(ComputeEvent::CodeProcessed)),
            ));
        }

        Poll::Pending
    }
}

impl FusedStream for ComputeService {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl ComputeService {
    // TODO #4550: consider to create Processor inside ComputeService
    pub fn new(db: Database, processor: Processor) -> Self {
        Self {
            db,
            processor,
            blocks_queue: VecDeque::new(),
            process_block: Default::default(),
            process_codes: Default::default(),
        }
    }

    pub fn receive_code(&mut self, code_id: CodeId, timestamp: u64, code: Vec<u8>) {
        let mut processor = self.processor.clone();
        self.process_codes.spawn_blocking(move || {
            let valid = processor.process_upload_code_raw(code_id, code.as_slice())?;
            Ok(CodeCommitment {
                id: code_id,
                timestamp,
                valid,
            })
        });
    }

    pub fn receive_synced_head(&mut self, block: H256) {
        if self.process_block.is_none() {
            let context = ChainHeadProcessContext {
                db: self.db.clone(),
                processor: self.processor.clone(),
            };

            self.process_block = Some(Box::pin(context.process(block)));
        } else {
            self.blocks_queue.push_back(block);
        }
    }
}

struct ChainHeadProcessContext {
    db: Database,
    processor: Processor,
}

impl ChainHeadProcessContext {
    /// Processes the chain of not computed blocks starting from the given `head`.
    ///
    /// If there is a chain of blocks to be processed, then `head` is the latest block.
    async fn process(mut self, head: H256) -> Result<BlockProcessed> {
        let chain = Self::collect_not_computed_blocks_chain(&self.db, head)?;

        // Bypass the chain in reverse order (from the oldest to the newest) and compute each block.
        for block_data in chain.into_iter().rev() {
            self.process_one_block(block_data).await?;
        }

        Ok(BlockProcessed { block_hash: head })
    }

    /// Processes events from the provided block.
    ///
    /// The processing is a complex task, which involves:
    /// - Instrumenting validated codes and setting the instrumented version, if not already done.
    /// - Building commitments from outcomes resulted from processing block events, which itself
    ///   returns state transition for ethexe actors.
    /// - Merging the fresh commitments with those from the parent block. Parent block commitments
    ///   can remain pending (not fully processed by the sequencer service).
    /// - Setting the block as computed, it's outcome from events processing and the final commitment queue.
    async fn process_one_block(&mut self, block_data: SimpleBlockData) -> Result<()> {
        let SimpleBlockData {
            hash: block,
            header,
        } = block_data;

        // Events must be set for all synced blocks.
        let events = OnChainStorage::block_events(&self.db, block)
            .ok_or_else(|| anyhow!("events not found for synced block {block}"))?;

        for event in &events {
            if let BlockEvent::Router(RouterEvent::CodeGotValidated {
                code_id,
                valid: true,
            }) = event
            {
                use ethexe_common::db::CodesStorage;
                if self.db.instrumented_code(0, *code_id).is_none() {
                    let code = CodesStorage::original_code(&self.db, *code_id)
                        .ok_or_else(|| anyhow!("code not found for validated code {code_id}"))?;
                    self.processor.process_upload_code(*code_id, &code)?;
                }
            }
        }

        let parent = header.parent_hash;

        if !self.db.block_computed(parent) {
            unreachable!("Parent block {parent} must be computed before the current one {block}",);
        }

        let mut commitments_queue =
            Self::propagate_data_from_parent(&self.db, block, parent, events.iter())?;

        let block_request_events = events
            .into_iter()
            .filter_map(|event| event.to_request())
            .collect();

        let block_outcomes = self
            .processor
            .process_block_events(block, block_request_events)?;

        let outcomes: Vec<_> = block_outcomes
            .into_iter()
            .map(|outcome| {
                // todo [sab] not needed, because the `process_block_events_raw` already returns the outcomes
                if let LocalOutcome::Transition(transition) = outcome {
                    transition
                } else {
                    unreachable!("Only transitions are expected here")
                }
            })
            .collect();

        if !outcomes.is_empty() {
            commitments_queue.push_back(block);
        }
        self.db.set_block_commitment_queue(block, commitments_queue);

        self.db.set_block_outcome(block, outcomes);

        // TODO #4551: move set_program_states here from processor

        // Set block as valid - means state db has all states for the end of the block
        self.db.set_block_computed(block);

        self.db.set_latest_computed_block(block, header);

        Ok(())
    }

    /// Gets `wait for commitment` blocks queue from the `parent` of the `block`.
    ///
    /// The returned data doesn't contain waiting for commitment blocks included
    /// into current (`block`) block.
    ///
    /// The `block` can have requests for code validation. These requests are united with those
    /// from the `parent` block and set to the `block`'s codes queue.
    fn propagate_data_from_parent<'a>(
        db: &Database,
        block: H256,
        parent: H256,
        events: impl Iterator<Item = &'a BlockEvent>,
    ) -> Result<VecDeque<H256>> {
        // Propagate prev commitment (prev not empty block hash or zero for genesis).
        if db
            .block_outcome_is_empty(parent)
            .ok_or_else(|| anyhow!("emptiness not found for computed block {parent}"))?
        {
            let parent_prev_commitment = db
                .previous_not_empty_block(parent)
                .ok_or_else(|| anyhow!("prev commitment not found for computed block {parent}"))?;
            db.set_previous_not_empty_block(block, parent_prev_commitment);
        } else {
            db.set_previous_not_empty_block(block, parent);
        }

        let mut committed_blocks_in_current = BTreeSet::new();
        let mut validated_codes_in_current = BTreeSet::new();
        let mut requested_codes_in_current = Vec::new();

        for event in events {
            match event {
                BlockEvent::Router(RouterEvent::BlockCommitted { hash }) => {
                    committed_blocks_in_current.insert(*hash);
                }
                BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, .. }) => {
                    validated_codes_in_current.insert(*code_id);
                }
                BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, .. }) => {
                    requested_codes_in_current.push(*code_id);
                }
                _ => {}
            }
        }

        // Propagate `wait for commitment` blocks queue
        let mut blocks_queue = db
            .block_commitment_queue(parent)
            .ok_or_else(|| anyhow!("commitment queue not found for computed block {parent}"))?;
        blocks_queue.retain(|hash| !committed_blocks_in_current.contains(hash));

        // Propagate `wait for code validation` blocks queue
        let mut codes_queue = db
            .block_codes_queue(parent)
            .ok_or_else(|| anyhow!("codes queue not found for computed block {parent}"))?;
        codes_queue.retain(|code_id| !validated_codes_in_current.contains(code_id));
        codes_queue.extend(requested_codes_in_current);
        db.set_block_codes_queue(block, codes_queue);

        Ok(blocks_queue)
    }

    /// Collect a chain of blocks from the head to the last not computed block.
    fn collect_not_computed_blocks_chain(
        db: &Database,
        head: H256,
    ) -> Result<Vec<SimpleBlockData>> {
        let mut block = head;
        let mut chain = vec![];
        while !db.block_computed(block) {
            if !db.block_is_synced(block) {
                return Err(anyhow!("Block {block} is not synced, but must be"));
            }

            // Headers must be set for all synced blocks.
            let header = OnChainStorage::block_header(db, block)
                .ok_or_else(|| anyhow!("header not found for synced block {block}"))?;

            let parent = header.parent_hash;

            chain.push(SimpleBlockData {
                hash: block,
                header,
            });

            block = parent;
        }

        Ok(chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_codes_queue_propagation() {
        let db = Database::memory();

        // Prepare test data
        let parent_block = H256::random();
        let current_block = H256::random();
        let code_id_1 = H256::random().into();
        let code_id_2 = H256::random().into();

        // Simulate parent block with a codes queue
        let mut parent_codes_queue = VecDeque::new();
        parent_codes_queue.push_back(code_id_1);
        db.set_block_codes_queue(parent_block, parent_codes_queue.clone());
        db.set_block_outcome(parent_block, Default::default());
        db.set_previous_not_empty_block(parent_block, H256::random());
        db.set_block_commitment_queue(parent_block, Default::default());

        // Simulate events for the current block
        let events = vec![
            BlockEvent::Router(RouterEvent::CodeGotValidated {
                code_id: code_id_1,
                valid: true,
            }),
            BlockEvent::Router(RouterEvent::CodeValidationRequested {
                code_id: code_id_2,
                timestamp: 0,
                tx_hash: H256::random(),
            }),
        ];
        db.set_block_events(current_block, &events);

        // Propagate data from parent
        ChainHeadProcessContext::propagate_data_from_parent(
            &db,
            current_block,
            parent_block,
            db.block_events(current_block).unwrap().iter(),
        )
        .unwrap();

        // Check for parent
        let codes_queue = db.block_codes_queue(parent_block).unwrap();
        assert_eq!(codes_queue, parent_codes_queue);

        // Check for current block
        let codes_queue = db.block_codes_queue(current_block).unwrap();
        assert_eq!(codes_queue, VecDeque::from(vec![code_id_2]));
    }
}
