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

use crate::{
    context::{BlockProcessed, ChainHeadProcessContext},
    precompute::PreCompute,
};
use anyhow::{anyhow, Result};
use ethexe_common::{db::BlockMetaStorage, gear::CodeCommitment};
use ethexe_db::Database;
use ethexe_processor::Processor;
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream};
use gprimitives::{CodeId, H256};
use std::{
    collections::{HashSet, VecDeque},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinSet;

pub mod context;
pub mod precompute;

#[derive(Debug, Clone)]
pub enum ComputeEvent {
    RequestLoadCodes(HashSet<CodeId>),
    BlockProcessed(BlockProcessed),
    CodeProcessed(CodeCommitment),
}

// TODO #4548: add state monitoring in prometheus
// TODO #4549: add tests for compute service
pub struct ComputeService {
    db: Database,
    processor: Processor,

    pre_compute: PreCompute,

    // ready_to_process: HashSet<H256>,
    blocks_to_process_queue: VecDeque<H256>,

    process_block: Option<BoxFuture<'static, Result<BlockProcessed>>>,
    process_codes: JoinSet<Result<CodeCommitment>>,
}

impl Stream for ComputeService {
    type Item = Result<ComputeEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(res)) = self.process_codes.poll_join_next(cx) {
            match res {
                Ok(Ok(commitment)) => {
                    self.pre_compute.receive_loaded_code(commitment.id);
                    return Poll::Ready(Some(Ok(ComputeEvent::CodeProcessed(commitment))));
                }
                Ok(Err(e)) => return Poll::Ready(Some(Err(anyhow!("process code error: {e}")))),
                Err(e) => return Poll::Ready(Some(Err(anyhow!("process codes join error: {e}")))),
            }
        }

        if let Some(fut) = self.process_block.as_mut() {
            if let Poll::Ready(res) = fut.as_mut().poll_unpin(cx) {
                self.process_block = None;
                let maybe_event = res.map(ComputeEvent::BlockProcessed);
                return Poll::Ready(Some(maybe_event));
            }
        }

        if let Poll::Ready(maybe_codes) = self.pre_compute.poll_unpin(cx) {
            return Poll::Ready(Some(maybe_codes.map(ComputeEvent::RequestLoadCodes)));
        }

        if let Some(block) = self.blocks_to_process_queue.back().copied() {
            if self.db.block_pre_computed(block) {
                let context = ChainHeadProcessContext {
                    db: self.db.clone(),
                    processor: self.processor.clone(),
                };
                self.process_block = Some(context.process(block).boxed());

                let _ = self.blocks_to_process_queue.pop_back();
                cx.waker().wake_by_ref();
            } else {
                self.pre_compute.pre_compute_block(block);
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
            db: db.clone(),
            processor,
            pre_compute: PreCompute::new(db),
            // ready_to_process: HashSet::new(),
            blocks_to_process_queue: VecDeque::new(),
            process_block: None,
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

    pub fn precompute_block(&mut self, block: H256) {
        self.pre_compute.pre_compute_block(block);
    }

    pub fn process_block(&mut self, block: H256) {
        self.blocks_to_process_queue.push_front(block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use context::*;
    use ethexe_common::{
        db::{BlockMetaStorage, OnChainStorage},
        events::{BlockEvent, RouterEvent},
    };
    // use precompute::*;

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
