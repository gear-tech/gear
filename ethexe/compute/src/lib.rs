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

use ethexe_common::{
    db::{BlockMetaStorageRead, BlockMetaStorageWrite, CodesStorageRead},
    CodeAndIdUnchecked, SimpleBlockData,
};
use ethexe_db::Database;
use ethexe_processor::{Processor, ProcessorError};
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream};
use gprimitives::{CodeId, H256};
use prepare::PrepareInfo;
use std::{
    collections::{HashSet, VecDeque},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinSet;

mod compute;
mod prepare;
mod utils;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BlockProcessed {
    pub block_hash: H256,
}

#[derive(Debug, Clone, Eq, PartialEq, derive_more::Unwrap)]
pub enum ComputeEvent {
    RequestLoadCodes(HashSet<CodeId>),
    CodeProcessed(CodeId),
    BlockPrepared(H256),
    BlockProcessed(BlockProcessed),
}

#[derive(thiserror::Error, Debug)]
pub enum ComputeError {
    #[error("block({0}) requested to process, but it's not prepared")]
    BlockNotPrepared(H256),
    #[error("block({0}) not synced")]
    BlockNotSynced(H256),
    #[error("not found events for block({0})")]
    BlockEventsNotFound(H256),
    #[error("block header not found for synced block({0})")]
    BlockHeaderNotFound(H256),
    #[error("process code join error")]
    CodeProcessJoin(#[from] tokio::task::JoinError),
    #[error("block outcome not set for computed block({0})")]
    ParentNotFound(H256),
    #[error("code({0}) marked as validated, but not found in db")]
    ValidatedCodeNotFound(CodeId),
    #[error("codes queue nоt found for computed block({0})")]
    CodesQueueNotFound(H256),
    #[error("commitment queue not found for computed block({0})")]
    CommitmentQueueNotFound(H256),
    #[error("previous commitment not found for computed block({0})")]
    PreviousCommitmentNotFound(H256),
    #[error("last committed batch not found for computed block({0})")]
    LastCommittedBatchNotFound(H256),
    #[error("code validation mismatch for code({code_id:?}), local status: {local_status}, remote status: {remote_status}")]
    CodeValidationStatusMismatch {
        code_id: CodeId,
        local_status: bool,
        remote_status: bool,
    },

    #[error(transparent)]
    Processor(#[from] ProcessorError),
}

type Result<T> = std::result::Result<T, ComputeError>;

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

// TODO #4548: add state monitoring in prometheus
// TODO #4549: add tests for compute service
pub struct ComputeService {
    db: Database,
    processor: Processor,

    blocks_queue: VecDeque<BlockAction>,
    blocks_state: State,

    process_codes: JoinSet<Result<CodeId>>,
}

impl ComputeService {
    // TODO #4550: consider to create Processor inside ComputeService
    pub fn new(db: Database, processor: Processor) -> Self {
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
                Ok(processor
                    .process_upload_code(code_and_id)
                    .map(|_valid| code_id)?)
            });
        }
    }

    pub fn prepare_block(&mut self, block: H256) {
        self.blocks_queue.push_front(BlockAction::Prepare(block));
    }

    pub fn process_block(&mut self, block: H256) {
        self.blocks_queue.push_front(BlockAction::Process(block));
    }
}

impl Stream for ComputeService {
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

                    return Poll::Ready(Some(Ok(ComputeEvent::RequestLoadCodes(missing_codes))));
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

impl FusedStream for ComputeService {
    fn is_terminated(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        db::{OnChainStorageRead, OnChainStorageWrite},
        events::{BlockEvent, RouterEvent},
        BlockHeader, Digest,
    };
    use futures::StreamExt;
    use gear_core::ids::prelude::CodeIdExt;
    use std::collections::HashMap;

    // Create new code with a unique nonce
    fn create_new_code(nonce: u32) -> Vec<u8> {
        let wat = format!(
            r#"(module
            (import "env" "memory" (memory 1))
            (export "init" (func $init))
            (func $init)
            (func $ret_{nonce}))"#,
        );

        let code = wat::parse_str(&wat).unwrap();
        wasmparser::validate(&code).unwrap();
        code
    }

    // Generate codes for the given chain and store the events in the database
    // Return a map with `CodeId` and corresponding code bytes
    fn generate_codes(
        db: Database,
        chain: &VecDeque<H256>,
        events_in_block: u32,
    ) -> HashMap<CodeId, Vec<u8>> {
        let mut nonce = 0;
        let mut codes_storage = HashMap::new();
        for block in chain.iter().copied() {
            let events: Vec<BlockEvent> = (0..events_in_block)
                .map(|_| {
                    nonce += 1;
                    let code = create_new_code(nonce);
                    let code_id = CodeId::generate(&code);
                    codes_storage.insert(code_id, code);

                    BlockEvent::Router(RouterEvent::CodeGotValidated {
                        code_id,
                        valid: true,
                    })
                })
                .collect();

            db.set_block_events(block, &events);
        }
        codes_storage
    }

    // Generate a chain with the given length and setup the genesis block
    fn generate_chain(db: Database, chain_len: u32) -> VecDeque<H256> {
        let genesis_hash = H256::from_low_u64_be(u64::MAX);
        db.set_block_codes_queue(genesis_hash, Default::default());
        db.mutate_block_meta(genesis_hash, |meta| {
            meta.computed = true;
            meta.prepared = true;
        });
        db.set_block_outcome(genesis_hash, vec![]);
        db.set_previous_not_empty_block(genesis_hash, H256::random());
        db.set_last_committed_batch(genesis_hash, Digest::random());
        db.set_block_commitment_queue(genesis_hash, Default::default());
        db.set_block_program_states(genesis_hash, Default::default());
        db.set_block_schedule(genesis_hash, Default::default());
        db.set_block_header(
            genesis_hash,
            BlockHeader {
                height: 0,
                timestamp: 0,
                parent_hash: H256::zero(),
            },
        );

        let mut chain = VecDeque::new();

        let mut parent_hash = genesis_hash;
        for block_num in 1..chain_len + 1 {
            let block_hash = H256::from_low_u64_be(block_num as u64);
            let block_header = BlockHeader {
                height: block_num,
                timestamp: (block_num * 10) as u64,
                parent_hash,
            };
            db.set_block_header(block_hash, block_header);
            db.mutate_block_meta(block_hash, |meta| meta.synced = true);
            chain.push_back(block_hash);
            parent_hash = block_hash;
        }

        chain
    }

    // A wrapper around the `ComputeService` to correctly handle code processing and block preparation
    struct WrappedComputeService {
        inner: ComputeService,
        codes_storage: HashMap<CodeId, Vec<u8>>,
    }

    impl WrappedComputeService {
        async fn prepare_and_assert_block(&mut self, block: H256) {
            self.inner.prepare_block(block);

            let event = self
                .inner
                .next()
                .await
                .unwrap()
                .expect("expect compute service request codes to load");
            let codes_to_load = event.unwrap_request_load_codes();

            for code_id in codes_to_load {
                // skip if code not validated
                let Some(code) = self.codes_storage.remove(&code_id) else {
                    continue;
                };

                self.inner
                    .process_code(CodeAndIdUnchecked { code, code_id });

                let event = self
                    .inner
                    .next()
                    .await
                    .unwrap()
                    .expect("expect code will be processing");
                let processed_code_id = event.unwrap_code_processed();

                assert_eq!(processed_code_id, code_id);
            }

            let event = self
                .inner
                .next()
                .await
                .unwrap()
                .expect("expect block prepared after processing all codes");
            let prepared_block = event.unwrap_block_prepared();
            assert_eq!(prepared_block, block);
        }

        async fn process_and_assert_block(&mut self, block: H256) {
            self.inner.process_block(block);

            let event = self
                .inner
                .next()
                .await
                .unwrap()
                .expect("expect block will be processing");

            let processed_block = event.unwrap_block_processed();
            assert_eq!(processed_block.block_hash, block);
        }
    }

    // Setup the chain and compute service.
    // It is needed to reduce the copy-paste in tests.
    fn setup_chain_and_compute(
        db: Database,
        chain_len: u32,
        events_in_block: u32,
    ) -> (VecDeque<H256>, WrappedComputeService) {
        let chain = generate_chain(db.clone(), chain_len);
        let codes_storage = generate_codes(db.clone(), &chain, events_in_block);

        let compute = WrappedComputeService {
            inner: ComputeService::new(db.clone(), Processor::new(db).unwrap()),
            codes_storage,
        };
        (chain, compute)
    }

    // #[tokio::test]
    // async fn test_codes_queue_propagation() {
    //     let db = Database::memory();

    //     // Prepare test data
    //     let parent_block = H256::random();
    //     let current_block = H256::random();
    //     let code_id_1 = H256::random().into();
    //     let code_id_2 = H256::random().into();

    //     // Simulate parent block with a codes queue
    //     let mut parent_codes_queue = VecDeque::new();
    //     parent_codes_queue.push_back(code_id_1);
    //     db.set_block_codes_queue(parent_block, parent_codes_queue.clone());
    //     db.set_block_outcome(parent_block, Default::default());
    //     db.set_previous_not_empty_block(parent_block, H256::random());
    //     db.set_last_committed_batch(parent_block, Digest::random());
    //     db.set_block_commitment_queue(parent_block, Default::default());

    //     // Simulate events for the current block
    //     let events = vec![
    //         BlockEvent::Router(RouterEvent::CodeGotValidated {
    //             code_id: code_id_1,
    //             valid: true,
    //         }),
    //         BlockEvent::Router(RouterEvent::CodeValidationRequested {
    //             code_id: code_id_2,
    //             timestamp: 0,
    //             tx_hash: H256::random(),
    //         }),
    //     ];
    //     db.set_block_events(current_block, &events);

    //     // Propagate data from parent
    //     ChainHeadProcessContext::propagate_data_from_parent(
    //         &db,
    //         current_block,
    //         parent_block,
    //         db.block_events(current_block).unwrap().iter(),
    //     )
    //     .unwrap();

    //     // Check for parent
    //     let codes_queue = db.block_codes_queue(parent_block).unwrap();
    //     assert_eq!(codes_queue, parent_codes_queue);

    //     // Check for current block
    //     let codes_queue = db.block_codes_queue(current_block).unwrap();
    //     assert_eq!(codes_queue, VecDeque::from(vec![code_id_2]));
    // }

    #[tokio::test]
    async fn block_computation_basic() -> Result<()> {
        gear_utils::init_default_logger();

        let chain_len = 1;
        let db = Database::memory();
        let (mut chain, mut compute) = setup_chain_and_compute(db, chain_len, 3);

        for _ in 0..chain_len {
            let block = chain.pop_front().unwrap();
            compute.prepare_and_assert_block(block).await;
            compute.process_and_assert_block(block).await;
        }

        Ok(())
    }

    #[tokio::test]
    async fn multiple_preparation_and_one_processing() -> Result<()> {
        gear_utils::init_default_logger();

        let chain_len = 3;
        let db = Database::memory();
        let (mut chain, mut compute) = setup_chain_and_compute(db, chain_len, 3);

        let block1 = chain.pop_front().unwrap();
        let block2 = chain.pop_front().unwrap();
        let block3 = chain.pop_front().unwrap();

        compute.prepare_and_assert_block(block1).await;
        compute.prepare_and_assert_block(block2).await;
        compute.prepare_and_assert_block(block3).await;

        compute.process_and_assert_block(block3).await;

        Ok(())
    }

    #[tokio::test]
    async fn one_preparation_and_multiple_processing() -> Result<()> {
        gear_utils::init_default_logger();

        let chain_len = 3;
        let db = Database::memory();
        let (mut chain, mut compute) = setup_chain_and_compute(db, chain_len, 3);

        let block1 = chain.pop_front().unwrap();
        let block2 = chain.pop_front().unwrap();
        let block3 = chain.pop_front().unwrap();

        compute.prepare_and_assert_block(block3).await;

        compute.process_and_assert_block(block1).await;
        compute.process_and_assert_block(block2).await;
        compute.process_and_assert_block(block3).await;

        Ok(())
    }

    #[tokio::test]
    async fn code_validation_request_does_not_block_preparation() -> Result<()> {
        gear_utils::init_default_logger();

        let chain_len = 1;
        let db = Database::memory();
        let (mut chain, mut compute) = setup_chain_and_compute(db.clone(), chain_len, 3);

        let block = chain.pop_back().unwrap();
        let mut block_events = db.block_events(block).unwrap();

        // add invalid event which shouldn't stop block preparation
        block_events.push(BlockEvent::Router(RouterEvent::CodeValidationRequested {
            code_id: CodeId::zero(),
            timestamp: 0u64,
            tx_hash: H256::random(),
        }));
        db.set_block_events(block, &block_events);

        compute.prepare_and_assert_block(block).await;
        compute.process_and_assert_block(block).await;

        Ok(())
    }
}
