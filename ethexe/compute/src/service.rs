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
    compute,
    prepare::{self, PrepareInfo},
    BlockProcessed, ComputeError, ComputeEvent, ProcessorExt, Result,
};
use ethexe_common::{
    db::{BlockMetaStorageRead, BlockMetaStorageWrite, CodesStorageRead},
    CodeAndIdUnchecked, SimpleBlockData,
};
use ethexe_db::Database;
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream};
use gprimitives::{CodeId, H256};
use std::{
    collections::{HashSet, VecDeque},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub struct ComputeMetrics {
    pub blocks_queue_len: usize,
    pub waiting_codes_count: usize,
    pub process_codes_count: usize,
}

#[derive(Debug, Clone)]
enum BlockAction {
    Prepare(H256),
    Process(H256),
}

#[derive(Default)]
enum State {
    #[default]
    WaitForBlock,
    WaitForCodes {
        block: H256,
        chain: VecDeque<SimpleBlockData>,
        waiting_codes: HashSet<CodeId>,
    },
    ComputeBlock(BoxFuture<'static, Result<BlockProcessed>>),
}

pub struct ComputeService<P: ProcessorExt> {
    db: Database,
    processor: P,

    blocks_queue: VecDeque<BlockAction>,
    blocks_state: State,

    process_codes: JoinSet<Result<CodeId>>,
}

impl<P: ProcessorExt> ComputeService<P> {
    // TODO #4550: consider to create Processor inside ComputeService
    pub fn new(db: Database, processor: P) -> Self {
        Self {
            db,
            processor,
            blocks_queue: Default::default(),
            blocks_state: State::WaitForBlock,
            process_codes: Default::default(),
        }
    }

    pub fn process_code(&mut self, code_and_id: CodeAndIdUnchecked) {
        let code_id = code_and_id.code_id;
        if let Some(valid) = self.db.code_valid(code_id) {
            // TODO: #4712 test this case
            log::warn!("Code {code_id:?} already processed");

            if valid {
                debug_assert!(
                    self.db.original_code_exists(code_id),
                    "Code {code_id:?} must exist in database"
                );
                debug_assert!(
                    self.db
                        .instrumented_code_exists(ethexe_runtime::VERSION, code_id),
                    "Instrumented code {code_id:?} must exist in database"
                );
            }

            self.process_codes.spawn(async move { Ok(code_id) });
        } else {
            let mut processor = self.processor.clone();

            self.process_codes.spawn_blocking(move || {
                processor
                    .process_upload_code(code_and_id)
                    .map(|_valid| code_id)
            });
        }
    }

    pub fn prepare_block(&mut self, block: H256) {
        self.blocks_queue.push_front(BlockAction::Prepare(block));
    }

    pub fn process_block(&mut self, block: H256) {
        self.blocks_queue.push_front(BlockAction::Process(block));
    }

    /// Get all metrics from the compute service
    pub fn get_metrics(&self) -> ComputeMetrics {
        let waiting_codes_count =
            if let State::WaitForCodes { waiting_codes, .. } = &self.blocks_state {
                waiting_codes.len()
            } else {
                0
            };

        ComputeMetrics {
            blocks_queue_len: self.blocks_queue.len(),
            waiting_codes_count,
            process_codes_count: self.process_codes.len(),
        }
    }
}

impl<P: ProcessorExt> Stream for ComputeService<P> {
    type Item = Result<ComputeEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(res)) = self.process_codes.poll_join_next(cx) {
            match res {
                Ok(res) => {
                    if let (Ok(code_id), State::WaitForCodes { waiting_codes, .. }) =
                        (&res, &mut self.blocks_state)
                    {
                        waiting_codes.remove(code_id);
                    }

                    return Poll::Ready(Some(res.map(ComputeEvent::CodeProcessed)));
                }
                Err(e) => return Poll::Ready(Some(Err(ComputeError::CodeProcessJoin(e)))),
            }
        }

        if matches!(self.blocks_state, State::WaitForBlock) {
            match self.blocks_queue.pop_back() {
                Some(BlockAction::Prepare(block)) => {
                    let PrepareInfo {
                        chain,
                        missing_codes,
                        missing_validated_codes,
                    } = prepare::prepare(&self.db, block)?;

                    self.blocks_state = State::WaitForCodes {
                        block,
                        chain,
                        waiting_codes: missing_validated_codes,
                    };

                    if !missing_codes.is_empty() {
                        return Poll::Ready(Some(Ok(ComputeEvent::RequestLoadCodes(
                            missing_codes,
                        ))));
                    }
                }
                Some(BlockAction::Process(block)) => {
                    if !self.db.block_meta(block).prepared {
                        return Poll::Ready(Some(Err(ComputeError::BlockNotPrepared(block))));
                    }

                    self.blocks_state = State::ComputeBlock(
                        compute::compute(self.db.clone(), self.processor.clone(), block).boxed(),
                    );
                }
                None => {}
            }
        }

        if let State::WaitForCodes {
            block,
            chain,
            waiting_codes,
        } = &self.blocks_state
        {
            if waiting_codes.is_empty() {
                // All codes are loaded, we can mark the block as prepared
                for block_data in chain {
                    self.db
                        .mutate_block_meta(block_data.hash, |meta| meta.prepared = true);
                }
                let event = ComputeEvent::BlockPrepared(*block);
                self.blocks_state = State::WaitForBlock;
                return Poll::Ready(Some(Ok(event)));
            }
        }

        if let State::ComputeBlock(future) = &mut self.blocks_state {
            if let Poll::Ready(res) = future.poll_unpin(cx) {
                self.blocks_state = State::WaitForBlock;
                return Poll::Ready(Some(res.map(ComputeEvent::BlockProcessed)));
            }
        }

        Poll::Pending
    }
}

impl<P: ProcessorExt> FusedStream for ComputeService<P> {
    fn is_terminated(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::MockProcessor;
    use ethexe_common::{
        db::{BlockMetaStorageWrite, OnChainStorageWrite},
        BlockHeader, CodeAndIdUnchecked,
    };
    use ethexe_db::Database as DB;
    use futures::StreamExt;
    use gear_core::ids::prelude::CodeIdExt;
    use gprimitives::{CodeId, H256};
    use std::collections::VecDeque;

    /// Test ComputeService block preparation functionality
    #[tokio::test]
    async fn test_compute_service_prepare_block() {
        let db = DB::memory();
        let processor = MockProcessor;
        let mut service = ComputeService::new(db.clone(), processor);

        let parent_hash = H256::from([1; 32]);
        let block_hash = H256::from([2; 32]);

        // Setup parent block as prepared
        db.mutate_block_meta(parent_hash, |meta| {
            meta.synced = true;
            meta.prepared = true;
        });
        db.set_last_committed_batch(parent_hash, Default::default());
        db.set_block_codes_queue(parent_hash, VecDeque::new());

        // Setup block as synced but not prepared
        db.mutate_block_meta(block_hash, |meta| {
            meta.synced = true;
            meta.prepared = false;
        });
        let header = BlockHeader {
            height: 2,
            parent_hash,
            timestamp: 2000,
        };
        db.set_block_header(block_hash, header);
        db.set_block_events(block_hash, &[]);

        // Request block preparation
        service.prepare_block(block_hash);

        // Poll service to process the preparation request
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(event, ComputeEvent::BlockPrepared(block_hash));

        // Verify block is marked as prepared in DB
        assert!(db.block_meta(block_hash).prepared);
    }

    /// Test ComputeService block processing functionality
    #[tokio::test]
    async fn test_compute_service_process_block() {
        let db = DB::memory();
        let processor = MockProcessor;
        let mut service = ComputeService::new(db.clone(), processor);

        let parent_hash = H256::from([1; 32]);
        let block_hash = H256::from([2; 32]);

        // Setup parent block as computed
        db.mutate_block_meta(parent_hash, |meta| meta.computed = true);
        db.set_block_commitment_queue(parent_hash, VecDeque::new());
        db.set_block_outcome(parent_hash, vec![]);
        db.set_previous_not_empty_block(parent_hash, parent_hash);

        // Setup block as prepared
        db.mutate_block_meta(block_hash, |meta| {
            meta.synced = true;
            meta.prepared = true;
        });
        let header = BlockHeader {
            height: 2,
            parent_hash,
            timestamp: 2000,
        };
        db.set_block_header(block_hash, header);
        db.set_block_events(block_hash, &[]);

        // Request block processing
        service.process_block(block_hash);

        // Poll service to process the block
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(
            event,
            ComputeEvent::BlockProcessed(BlockProcessed { block_hash })
        );

        // Verify block is marked as computed in DB
        assert!(db.block_meta(block_hash).computed);
    }

    /// Test ComputeService code processing functionality
    #[tokio::test]
    async fn test_compute_service_process_code() {
        let db = DB::memory();
        let processor = MockProcessor;
        let mut service = ComputeService::new(db.clone(), processor);

        // Create test code
        let code = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]; // Simple WASM header
        let code_id = CodeId::generate(&code);

        let code_and_id = CodeAndIdUnchecked { code, code_id };

        // Verify code is not yet in DB
        assert!(db.code_valid(code_id).is_none());

        // Request code processing
        service.process_code(code_and_id);

        // Poll service to process the code
        let event = service.next().await.unwrap().unwrap();

        // Should receive CodeProcessed event with correct code_id
        assert_eq!(event, ComputeEvent::CodeProcessed(code_id));
    }
}
