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
    db::{BlockMetaStorage, BlocksOnChainData},
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

#[derive(Debug)]
pub struct BlockProcessed {
    pub block_hash: H256,
}

#[derive(Debug)]
pub enum ComputeEvent {
    BlockProcessed(BlockProcessed),
    CodeProcessed(CodeCommitment),
}

// TODO (gsobol): add state monitoring in prometheus
// TODO (gsobol): append off-chain transactions handling
// TODO (gsobol) asap: add tests for compute service
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
    // TODO (gsobol): consider to create Processor inside ComputeService
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
    fn propagate_data_from_parent<'a>(
        db: &Database,
        block: H256,
        parent: H256,
        events: impl Iterator<Item = &'a BlockEvent>,
    ) -> Result<VecDeque<H256>> {
        // Propagate program state hashes
        let state_hashes = db
            .block_end_program_states(parent)
            .ok_or_else(|| anyhow!("program states not found for computed block {parent}"))?;
        db.set_block_start_program_states(block, state_hashes);

        // Propagate scheduled tasks
        let schedule = db
            .block_end_schedule(parent)
            .ok_or_else(|| anyhow!("scheduled tasks not found for computed block {parent}"))?;
        db.set_block_start_schedule(block, schedule);

        // Propagate prev commitment (prev not empty block hash or zero for genesis).
        if db
            .block_is_empty(parent)
            .ok_or_else(|| anyhow!("emptiness not found for computed block {parent}"))?
        {
            let parent_prev_commitment = db
                .previous_committed_block(parent)
                .ok_or_else(|| anyhow!("prev commitment not found for computed block {parent}"))?;
            db.set_previous_committed_block(block, parent_prev_commitment);
        } else {
            db.set_previous_committed_block(block, parent);
        }

        // Propagate `wait for commitment` blocks queue
        let mut queue = db
            .block_commitment_queue(parent)
            .ok_or_else(|| anyhow!("commitment queue not found for computed block {parent}"))?;
        let committed_blocks_in_current: BTreeSet<_> = events
            .filter_map(|event| match event {
                BlockEvent::Router(RouterEvent::BlockCommitted { hash }) => Some(*hash),
                _ => None,
            })
            .collect();
        queue.retain(|hash| !committed_blocks_in_current.contains(hash));

        Ok(queue)
    }

    /// Collect a chain of blocks from the head to the last not computed block.
    fn collect_not_computed_blocks_chain(
        db: &Database,
        head: H256,
    ) -> Result<Vec<SimpleBlockData>> {
        let mut block = head;
        let mut chain = vec![];
        while !db.block_end_state_is_valid(block).unwrap_or(false) {
            if !db.block_is_synced(block) {
                return Err(anyhow!("Block {block} is not synced, but must be"));
            }

            let header = BlocksOnChainData::block_header(db, block)
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

    async fn process_one_block(&mut self, block_data: SimpleBlockData) -> Result<()> {
        let SimpleBlockData {
            hash: block,
            header,
        } = block_data;

        let events = BlocksOnChainData::block_events(&self.db, block)
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

        if !self.db.block_end_state_is_valid(parent).unwrap_or(false) {
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
                if let LocalOutcome::Transition(transition) = outcome {
                    transition
                } else {
                    unreachable!("Only transitions are expected here")
                }
            })
            .collect();

        self.db.set_block_is_empty(block, outcomes.is_empty());

        if !outcomes.is_empty() {
            commitments_queue.push_back(block);
        }
        self.db.set_block_commitment_queue(block, commitments_queue);

        self.db.set_block_outcome(block, outcomes);

        // TODO (gsobol): move set_program_states here from processor

        // Set block as valid - means state db has all states for the end of the block
        self.db.set_block_end_state_is_valid(block, true);

        self.db.set_latest_valid_block(block, header);

        Ok(())
    }

    async fn process(mut self, head: H256) -> Result<BlockProcessed> {
        let chain = Self::collect_not_computed_blocks_chain(&self.db, head)?;

        // Bypass the chain in reverse order (from the oldest to the newest) and compute each block.
        for block_data in chain.into_iter().rev() {
            self.process_one_block(block_data).await?;
        }

        Ok(BlockProcessed { block_hash: head })
    }
}
