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

use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage, OnChainStorage},
    events::{BlockEvent, RouterEvent},
    gear::CodeCommitment,
    CodeInfo, SimpleBlockData,
};
use ethexe_db::Database;
use ethexe_processor::{BlockProcessingResult, Processor};
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream};
use gprimitives::{CodeId, H256};
use std::{
    collections::{BTreeSet, HashSet, VecDeque},
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
    RequestLoadCodes(HashSet<CodeId>),
    BlockProcessed(BlockProcessed),
    CodeProcessed(CodeCommitment),
}

enum BlockPreparationState {
    NotStarted,
    WaitForCodes(HashSet<CodeId>),
}

// TODO #4548: add state monitoring in prometheus
// TODO #4549: add tests for compute service
pub struct ComputeService {
    db: Database,
    processor: Processor,

    blocks_queue: VecDeque<H256>,
    blocks_process_queue: VecDeque<H256>,

    state: BlockPreparationState,
    process_block_future: Option<BoxFuture<'static, Result<BlockProcessed>>>,
    process_codes: JoinSet<Result<CodeCommitment>>,
}

impl Stream for ComputeService {
    type Item = Result<ComputeEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(res)) = self.process_codes.poll_join_next(cx) {
            match res {
                Ok(Ok(commitment)) => {
                    if let BlockPreparationState::WaitForCodes(waiting_codes) = &mut self.state {
                        if waiting_codes.contains(&commitment.id) {
                            waiting_codes.remove(&commitment.id);
                        }
                    }
                    return Poll::Ready(Some(Ok(ComputeEvent::CodeProcessed(commitment))));
                }
                Ok(Err(e)) => return Poll::Ready(Some(Err(anyhow!("process code error: {e}")))),
                Err(e) => return Poll::Ready(Some(Err(anyhow!("process codes join error: {e}")))),
            }
        }

        if let BlockPreparationState::WaitForCodes(waiting_codes) = &self.state {
            if waiting_codes.is_empty() {
                self.state = BlockPreparationState::NotStarted;
            }
        }

        match self.process_block_future.as_mut() {
            Some(fut) => match fut.as_mut().poll_unpin(cx) {
                Poll::Ready(res) => {
                    let maybe_event = res.map(ComputeEvent::BlockProcessed);
                    self.process_block_future = None;
                    return Poll::Ready(Some(maybe_event));
                }
                Poll::Pending => return Poll::Pending,
            },
            None => {
                if let Some(block) = self.blocks_process_queue.pop_back() {
                    let fut = ChainHeadProcessContext {
                        db: self.db.clone(),
                        processor: self.processor.clone(),
                    }
                    .process(block)
                    .boxed();

                    self.process_block_future = Some(fut);
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }

                if let Some(block) = self.blocks_queue.pop_back() {
                    let (validated_codes, codes_to_load) = self.collect_chain_codes(block)?;
                    self.state = BlockPreparationState::WaitForCodes(validated_codes);

                    return Poll::Ready(Some(Ok(ComputeEvent::RequestLoadCodes(codes_to_load))));
                }
            }
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
            blocks_process_queue: VecDeque::new(),
            state: BlockPreparationState::NotStarted,
            process_block_future: None,
            process_codes: Default::default(),
        }
    }

    pub fn process_code(&mut self, code_id: CodeId, timestamp: u64, code: Vec<u8>) {
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

    pub fn prepare_block_to_compute(&mut self, block: H256) {
        self.blocks_queue.push_front(block);
    }

    pub fn process_block(&mut self, block: H256) {
        self.blocks_process_queue.push_front(block);
    }

    fn collect_chain_codes(&self, block: H256) -> Result<(HashSet<CodeId>, HashSet<CodeId>)> {
        let chain = ChainHeadProcessContext::collect_not_computed_blocks_chain(&self.db, block)?;

        let mut validated_codes = HashSet::new();
        let mut codes_to_load = HashSet::new();
        for block in chain {
            let (block_validated_coded, block_codes_to_load) =
                self.collect_block_codes(block.hash)?;

            validated_codes.extend(block_validated_coded.into_iter());
            codes_to_load.extend(block_codes_to_load.into_iter());
        }

        Ok((validated_codes, codes_to_load))
    }

    fn collect_block_codes(&self, block: H256) -> Result<(HashSet<CodeId>, HashSet<CodeId>)> {
        let events = self
            .db
            .block_events(block)
            .ok_or(anyhow!("observer must set block events"))?;

        let mut validated_codes = HashSet::new();
        let mut requested_codes = HashSet::new();

        for event in &events {
            match event {
                BlockEvent::Router(RouterEvent::CodeValidationRequested {
                    code_id,
                    timestamp,
                    tx_hash,
                }) => {
                    let code_info = CodeInfo {
                        timestamp: *timestamp,
                        tx_hash: *tx_hash,
                    };
                    self.db.set_code_blob_info(*code_id, code_info.clone());

                    if !self.db.original_code_exists(*code_id) && !validated_codes.contains(code_id)
                    {
                        requested_codes.insert(*code_id);
                    }
                }
                BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, .. }) => {
                    if requested_codes.contains(code_id) {
                        return Err(anyhow!("Code {code_id} is validated before requested"));
                    };

                    if !self.db.original_code_exists(*code_id) {
                        validated_codes.insert(*code_id);
                    }
                }
                _ => {}
            }
        }

        // Return validated codes and all codes to load
        requested_codes.extend(validated_codes.iter());
        Ok((validated_codes, requested_codes))
    }
}

struct ChainHeadProcessContext {
    db: Database,
    processor: Processor,
}

impl ChainHeadProcessContext {
    async fn process(mut self, head: H256) -> Result<BlockProcessed> {
        let chain = Self::collect_not_computed_blocks_chain(&self.db, head)?;

        // Bypass the chain in reverse order (from the oldest to the newest) and compute each block.
        for block_data in chain.into_iter().rev() {
            self.process_one_block(block_data).await?;
        }
        Ok(BlockProcessed { block_hash: head })
    }

    async fn process_one_block(&mut self, block_data: SimpleBlockData) -> Result<()> {
        let SimpleBlockData {
            hash: block,
            header,
        } = block_data;

        let events = OnChainStorage::block_events(&self.db, block)
            .ok_or_else(|| anyhow!("events not found for synced block {block}"))?;

        for event in &events {
            if let BlockEvent::Router(RouterEvent::CodeGotValidated {
                code_id,
                valid: true,
            }) = event
            {
                // TODO: test branch
                if !self
                    .db
                    .instrumented_code_exists(ethexe_runtime::VERSION, *code_id)
                {
                    let code = self
                        .db
                        .original_code(*code_id)
                        .ok_or(anyhow!("code not found for validated code {code_id}"))?;
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

        let processing_result = self
            .processor
            .process_block_events(block, block_request_events)?;

        let BlockProcessingResult {
            transitions,
            states,
            schedule,
        } = processing_result;

        if !transitions.is_empty() {
            commitments_queue.push_back(block);
        }
        self.db.set_block_commitment_queue(block, commitments_queue);

        self.db.set_block_outcome(block, transitions);

        self.db.set_block_program_states(block, states);
        self.db.set_block_schedule(block, schedule);

        // Set block as valid - means state db has all states for the end of the block
        self.db.set_block_computed(block);

        self.db.set_latest_computed_block(block, header);

        Ok(())
    }

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
