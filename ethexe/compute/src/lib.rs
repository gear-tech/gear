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
    db::{BlockMetaStorageRead, BlockMetaStorageWrite, CodesStorageRead, OnChainStorageRead},
    events::{BlockEvent, RouterEvent},
    SimpleBlockData,
};
use ethexe_db::Database;
use ethexe_processor::{BlockProcessingResult, Processor, ProcessorError};
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream};
use gprimitives::{CodeId, H256};
use std::{
    collections::{BTreeSet, HashSet, VecDeque},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinSet;

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
    #[error("previous commitment not found for computed block ({0})")]
    PreviousCommitmentNotFound(H256),

    #[error(transparent)]
    Processor(#[from] ProcessorError),
}

type Result<T> = std::result::Result<T, ComputeError>;

#[derive(Debug, Clone)]
enum BlockAction {
    Prepare(H256),
    Process(H256),
}

#[derive(Debug, PartialEq, Eq)]
enum BlockPreparationState {
    WaitForBlock,
    WaitForCodes {
        block: H256,
        chain: Vec<SimpleBlockData>,
        waiting_codes: HashSet<CodeId>,
    },
}

// TODO #4548: add state monitoring in prometheus
// TODO #4549: add tests for compute service
pub struct ComputeService {
    db: Database,
    processor: Processor,

    blocks_queue: VecDeque<BlockAction>,
    state: BlockPreparationState,

    process_block: Option<BoxFuture<'static, Result<BlockProcessed>>>,
    process_codes: JoinSet<Result<CodeId>>,
}

impl Stream for ComputeService {
    type Item = Result<ComputeEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(res)) = self.process_codes.poll_join_next(cx) {
            match res {
                Ok(res) => {
                    if let (
                        Ok(code_id),
                        BlockPreparationState::WaitForCodes { waiting_codes, .. },
                    ) = (&res, &mut self.state)
                    {
                        waiting_codes.remove(code_id);
                    }

                    return Poll::Ready(Some(res.map(ComputeEvent::CodeProcessed)));
                }
                Err(e) => return Poll::Ready(Some(Err(ComputeError::CodeProcessJoin(e)))),
            }
        }

        if self.process_block.is_none() && self.state == BlockPreparationState::WaitForBlock {
            match self.blocks_queue.pop_back() {
                Some(BlockAction::Prepare(block)) => {
                    let (chain, validated_codes, codes_to_load) =
                        self.collect_chain_codes(block)?;

                    self.state = BlockPreparationState::WaitForCodes {
                        block,
                        chain,
                        waiting_codes: validated_codes,
                    };
                    return Poll::Ready(Some(Ok(ComputeEvent::RequestLoadCodes(codes_to_load))));
                }
                Some(BlockAction::Process(block)) => {
                    if !self.db.block_meta(block).prepared {
                        return Poll::Ready(Some(Err(ComputeError::BlockNotPrepared(block))));
                    }

                    let context = ChainHeadProcessContext {
                        db: self.db.clone(),
                        processor: self.processor.clone(),
                    };
                    self.process_block = Some(context.process(block).boxed());
                }
                None => {}
            }
        }

        if let BlockPreparationState::WaitForCodes {
            block,
            chain,
            waiting_codes,
        } = &self.state
            && waiting_codes.is_empty()
        {
            for block_data in chain {
                self.db
                    .mutate_block_meta(block_data.hash, |meta| meta.prepared = true);
            }

            let event = ComputeEvent::BlockPrepared(*block);
            self.state = BlockPreparationState::WaitForBlock;
            return Poll::Ready(Some(Ok(event)));
        }

        if let Some(fut) = self.process_block.as_mut()
            && let Poll::Ready(res) = fut.poll_unpin(cx)
        {
            self.process_block = None;
            return Poll::Ready(Some(res.map(ComputeEvent::BlockProcessed)));
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
            blocks_queue: Default::default(),
            state: BlockPreparationState::WaitForBlock,
            process_block: None,
            process_codes: Default::default(),
        }
    }

    pub fn process_code(&mut self, code_id: CodeId, _timestamp: u64, code: Vec<u8>) {
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
                    .process_upload_code_raw(code_id, &code)
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

    fn collect_chain_codes(
        &self,
        block: H256,
    ) -> Result<(Vec<SimpleBlockData>, HashSet<CodeId>, HashSet<CodeId>)> {
        let chain = ChainHeadProcessContext::collect_not_computed_blocks_chain(&self.db, block)?;

        let mut validated_codes = HashSet::new();
        let mut codes_to_load = HashSet::new();
        for block in chain.iter() {
            let (block_validated_coded, block_codes_to_load) =
                self.collect_block_codes(block.hash)?;

            validated_codes.extend(block_validated_coded.into_iter());
            codes_to_load.extend(block_codes_to_load.into_iter());
        }

        Ok((chain, validated_codes, codes_to_load))
    }

    fn collect_block_codes(&self, block: H256) -> Result<(HashSet<CodeId>, HashSet<CodeId>)> {
        let events = self
            .db
            .block_events(block)
            .ok_or(ComputeError::BlockEventsNotFound(block))?;

        let mut validated_codes = HashSet::new();
        let mut codes_to_load = HashSet::new();

        for event in &events {
            match event {
                BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, .. }) => {
                    codes_to_load.insert(*code_id);
                }
                BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, .. }) => {
                    if !self.db.original_code_exists(*code_id) {
                        validated_codes.insert(*code_id);
                        codes_to_load.insert(*code_id);
                    }
                }
                _ => {}
            }
        }

        Ok((validated_codes, codes_to_load))
    }
}

struct ChainHeadProcessContext<
    DB: OnChainStorageRead + BlockMetaStorageWrite + BlockMetaStorageRead,
> {
    db: DB,
    processor: Processor,
}

impl<DB: OnChainStorageRead + BlockMetaStorageWrite + BlockMetaStorageRead>
    ChainHeadProcessContext<DB>
{
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

        let events = self
            .db
            .block_events(block)
            .ok_or(ComputeError::BlockEventsNotFound(block))?;

        let parent = header.parent_hash;
        if !self.db.block_meta(parent).computed {
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
            .process_block_events(block, block_request_events)
            .await?;

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
        self.db
            .mutate_block_meta(block, |meta| meta.computed = true);
        self.db.set_latest_computed_block(block, header);

        Ok(())
    }

    fn propagate_data_from_parent<'a>(
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
            .ok_or(ComputeError::CommitmentQueueNotFound(parent))?;
        blocks_queue.retain(|hash| !committed_blocks_in_current.contains(hash));

        // Propagate `wait for code validation` blocks queue
        let mut codes_queue = db
            .block_codes_queue(parent)
            .ok_or(ComputeError::CodesQueueNotFound(parent))?;
        codes_queue.retain(|code_id| !validated_codes_in_current.contains(code_id));
        codes_queue.extend(requested_codes_in_current);
        db.set_block_codes_queue(block, codes_queue);

        Ok(blocks_queue)
    }

    /// Collect a chain of blocks from the head to the last not computed block.
    fn collect_not_computed_blocks_chain(db: &DB, head: H256) -> Result<Vec<SimpleBlockData>> {
        let mut block = head;
        let mut chain = vec![];

        // Optimization to avoid double fetching of block meta.
        let mut block_meta = db.block_meta(block);
        while !block_meta.computed {
            if !block_meta.synced {
                return Err(ComputeError::BlockNotSynced(block));
            }

            let header = db
                .block_header(block)
                .ok_or(ComputeError::BlockHeaderNotFound(block))?;

            let parent = header.parent_hash;

            chain.push(SimpleBlockData {
                hash: block,
                header,
            });

            block = parent;
            block_meta = db.block_meta(block);
        }

        Ok(chain)
    }
}

#[cfg(test)]
mod tests {
    use ethexe_common::BlockHeader;
    use futures::StreamExt;
    use gear_core::ids::prelude::CodeIdExt;
    use std::collections::HashMap;

    use super::*;
    use ethexe_common::db::OnChainStorageWrite;

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
        let genesis_hash = H256::random();
        db.set_block_codes_queue(genesis_hash, Default::default());
        db.mutate_block_meta(genesis_hash, |meta| meta.computed = true);
        db.set_block_outcome(genesis_hash, vec![]);
        db.set_previous_not_empty_block(genesis_hash, H256::random());
        db.set_block_commitment_queue(genesis_hash, Default::default());
        db.set_block_program_states(genesis_hash, Default::default());
        db.set_block_schedule(genesis_hash, Default::default());

        let mut chain = VecDeque::new();

        let mut parent_hash = genesis_hash;
        for block_num in 0..chain_len {
            let block_hash = H256::random();
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

                self.inner.process_code(code_id, 0u64, code);

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
