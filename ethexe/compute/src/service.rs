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
    ComputeError, ComputeEvent, ProcessorExt, Result,
    compute::{self, ComputationStatus},
    prepare::{PrepareContext, PrepareStatus},
};
use ethexe_common::{
    Announce, BlockData, CheckedAnnouncesResponse, CodeAndIdUnchecked, SimpleBlockData,
    db::CodesStorageRead,
    db::BlockMetaStorageRead,
    db::OnChainStorageRead,
};
use ethexe_db::Database;
use ethexe_processor::Processor;
use futures::{
    FutureExt, Stream, channel,
    future::{BoxFuture, ready},
    stream::FusedStream,
};
use gprimitives::{CodeId, H256};
use std::{
    collections::{HashSet, VecDeque},
    hash::Hash,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub struct ComputeMetrics {
    pub blocks_queue_len: usize,
    pub process_codes_count: usize,
    pub waiting_codes_count: usize,
}

#[derive(Debug, Clone)]
enum BlockAction {
    Prepare(H256),
    Compute(Announce),
}

#[allow(clippy::large_enum_variant)]
#[derive(Default, derive_more::Debug)]
enum State {
    #[default]
    WaitForBlock,
    PreparePhase1(PrepareContext),
    PreparePhase2(#[debug(skip)] BoxFuture<'static, Result<H256>>),
    Computation(#[debug(skip)] BoxFuture<'static, Result<ComputationStatus>>),
}

pub struct ComputeService<P: ProcessorExt = Processor> {
    db: Database,
    processor: P,

    blocks_queue: VecDeque<BlockAction>,
    blocks_state: State,

    process_codes: JoinSet<Result<CodeId>>,
}

struct CodesSubService<P: ProcessorExt = Processor> {
    db: Database,
    processor: P,

    processions: JoinSet<Result<CodeId>>,
}

impl<P: ProcessorExt> CodesSubService<P> {
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
                        .instrumented_code_exists(ethexe_runtime_common::VERSION, code_id),
                    "Instrumented code {code_id:?} must exist in database"
                );
            }

            self.processions.spawn(async move { Ok(code_id) });
        } else {
            let mut processor = self.processor.clone();

            self.processions.spawn_blocking(move || {
                processor
                    .process_upload_code(code_and_id)
                    .map(|_valid| code_id)
            });
        }
    }
}

impl Stream for CodesSubService {
    type Item = Result<CodeId>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        futures::ready!(self.processions.poll_join_next(cx))
            .map(|res| res.map_err(|e| ComputeError::CodeProcessJoin(e))?)
            .map_or(Poll::Pending, |res| Poll::Ready(Some(res)))
    }
}

enum PrepareState {
    WaitingForBlock,
    WaitingForCodes {
        codes: HashSet<CodeId>,
        not_processed_blocks_queue: VecDeque<BlockData>,
    },
}

struct PrepareSubService {
    db: Database,

    blocks_queue: VecDeque<H256>,
    state: PrepareState,
}

impl PrepareSubService {
    pub fn receive_block_to_prepare(&mut self, block: H256) {
        self.blocks_queue.push_front(block);
    }

    pub fn receive_processed_code(&mut self, code_id: CodeId) {
        if let PrepareState::WaitingForCodes { codes, .. } = &mut self.state {
            codes.remove(&code_id);
        }
    }

    fn prepare(queue: VecDeque<BlockData>) -> Result<()> {
        for _block in queue {
            todo!();
        }

        Ok(())
    }
}

impl Stream for PrepareSubService {
    type Item = Result<ComputeEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let block_ready_to_prepare = match &mut self.state {
            PrepareState::WaitingForBlock => {
                let mut block_hash = match self.blocks_queue.pop_back() {
                    Some(block_hash) => block_hash,
                    None => return Poll::Pending,
                };

                if !self.db.block_synced(block_hash) {
                    return Poll::Ready(Some(Err(ComputeError::BlockNotSynced(block_hash))));
                }

                let mut queue = VecDeque::new();
                loop {
                    if self.db.block_meta(block_hash).prepared {
                        break;
                    }

                    let header = self.db.block_header(block_hash).ok_or({
                        ComputeError::BlockHeaderNotFound(block_hash)
                    })?;
                    let events = self.db.block_events(block_hash).ok_or({
                        ComputeError::BlockEventsNotFound(block_hash)
                    })?;

                    queue.push_front(BlockData {
                        hash: block_hash,
                        header,
                        events,
                    });
                }

                // collect missing and required codes. if required codes 
                todo!();
            },
            PrepareState::WaitingForCodes { codes, not_processed_blocks_queue } => {
                if !codes.is_empty() {
                    return Poll::Pending;
                }

                
            }
        }

        Poll::Pending
    }
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
                        .instrumented_code_exists(ethexe_runtime_common::VERSION, code_id),
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

    pub fn compute_announce(&mut self, announce: Announce) {
        self.blocks_queue.push_front(BlockAction::Compute(announce));
    }

    pub fn receive_announces_response(&mut self, response: CheckedAnnouncesResponse) {
        if let State::PreparePhase1(ctx) = &mut self.blocks_state {
            ctx.receive_announces(response);
        } else {
            log::warn!("Received announces response in unexpected state");
        }
    }

    /// Get all metrics from the compute service
    pub fn get_metrics(&self) -> ComputeMetrics {
        // +_+_+ fix
        // let waiting_codes_count =
        //     if let State::WaitForRequiredData { data, .. } = &self.blocks_state {
        //         codes.len()
        //     } else {
        //         0
        //     };

        ComputeMetrics {
            blocks_queue_len: self.blocks_queue.len(),
            process_codes_count: self.process_codes.len(),
            waiting_codes_count: 0,
        }
    }

    pub fn processor(&self) -> &P {
        &self.processor
    }
}

impl<P: ProcessorExt> Stream for ComputeService<P> {
    type Item = Result<ComputeEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(res)) = self.process_codes.poll_join_next(cx) {
            match res {
                Ok(res) => {
                    if let (Ok(code_id), State::PreparePhase1(ctx)) = (&res, &mut self.blocks_state)
                    {
                        ctx.receive_processed_code(*code_id);
                    }

                    return Poll::Ready(Some(res.map(ComputeEvent::CodeProcessed)));
                }
                Err(e) => return Poll::Ready(Some(Err(ComputeError::CodeProcessJoin(e)))),
            }
        }

        if let State::WaitForBlock = &self.blocks_state {
            match self.blocks_queue.pop_back() {
                Some(BlockAction::Prepare(block_hash)) => {
                    let (ctx, request) = PrepareContext::new(self.db.clone(), 3, block_hash)?;

                    self.blocks_state = State::PreparePhase1(ctx);

                    if !request.is_empty() {
                        return Poll::Ready(Some(Ok(ComputeEvent::RequestData(request))));
                    }
                }
                Some(BlockAction::Compute(announce)) => {
                    let future = compute::compute_and_include(
                        self.db.clone(),
                        self.processor.clone(),
                        announce,
                    )
                    .boxed();
                    self.blocks_state = State::Computation(future);
                }
                None => {}
            }
        }

        if let State::PreparePhase1(ctx) = &mut self.blocks_state {
            match ctx.prepare_if_ready() {
                Err(err) => {
                    self.blocks_state = State::WaitForBlock;
                    return Poll::Ready(Some(Err(err)));
                }
                Ok(PrepareStatus::Prepared(block_hash)) => {
                    self.blocks_state = State::PreparePhase2(
                        compute::compute_block_announces(
                            self.db.clone(),
                            self.processor.clone(),
                            block_hash,
                        )
                        .boxed(),
                    );
                }
                Ok(PrepareStatus::NotReady) => {}
            }
        }

        if let State::PreparePhase2(future) = &mut self.blocks_state
            && let Poll::Ready(res) = future.poll_unpin(cx)
        {
            self.blocks_state = State::WaitForBlock;
            return Poll::Ready(Some(res.map(ComputeEvent::BlockPrepared)));
        }

        if let State::Computation(future) = &mut self.blocks_state
            && let Poll::Ready(res) = future.poll_unpin(cx)
        {
            self.blocks_state = State::WaitForBlock;
            return Poll::Ready(Some(res.map(|status| match status {
                ComputationStatus::Computed(announce_hash) => {
                    ComputeEvent::AnnounceComputed(announce_hash, true)
                }
                ComputationStatus::Rejected(announce_hash) => {
                    ComputeEvent::AnnounceComputed(announce_hash, false)
                }
            })));
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
    use ethexe_common::{BlockHeader, CodeAndIdUnchecked, SimpleBlockData, db::*, mock::*};
    use ethexe_db::Database as DB;
    use futures::StreamExt;
    use gear_core::ids::prelude::CodeIdExt;
    use gprimitives::CodeId;

    /// Test ComputeService block preparation functionality
    #[tokio::test]
    async fn prepare_block() {
        gear_utils::init_default_logger();

        let db = DB::memory();
        let processor = MockProcessor;
        let mut service = ComputeService::new(db.clone(), processor);
        let chain = BlockChain::mock(1).setup(&db);

        let block = SimpleBlockData {
            hash: [2; 32].into(),
            header: BlockHeader {
                height: chain.blocks[1].as_synced().header.height + 1,
                parent_hash: chain.blocks[1].hash,
                timestamp: chain.blocks[1].as_synced().header.timestamp + 1000,
            },
        }
        .setup(&db);

        // Request block preparation
        service.prepare_block(block.hash);

        // Poll service to process the preparation request
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(event, ComputeEvent::BlockPrepared(block.hash));

        // Verify block is marked as prepared in DB
        assert!(db.block_meta(block.hash).prepared);
    }

    /// Test ComputeService block processing functionality
    #[tokio::test]
    async fn compute_announce() {
        gear_utils::init_default_logger();

        let db = DB::memory();
        let processor = MockProcessor;
        let mut service = ComputeService::new(db.clone(), processor);
        let chain = BlockChain::mock(1).setup(&db);

        let block = SimpleBlockData {
            hash: [2; 32].into(),
            header: BlockHeader {
                height: chain.blocks[1].as_synced().header.height + 1,
                parent_hash: chain.blocks[1].hash,
                timestamp: chain.blocks[1].as_synced().header.timestamp + 1000,
            },
        }
        .setup(&db);

        service.prepare_block(block.hash);
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(event, ComputeEvent::BlockPrepared(block.hash));

        // Request computation
        let announce = Announce {
            block_hash: block.hash,
            parent: chain.blocks[1]
                .as_prepared()
                .announces
                .first()
                .copied()
                .unwrap(),
            gas_allowance: Some(42),
            off_chain_transactions: vec![],
        };
        let announce_hash = announce.to_hash();
        service.compute_announce(announce);

        // Poll service to process the block
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(event, ComputeEvent::AnnounceComputed(announce_hash, true));

        // Verify block is marked as computed in DB
        assert!(db.announce_meta(announce_hash).computed);
    }

    /// Test ComputeService code processing functionality
    #[tokio::test]
    async fn process_code() {
        gear_utils::init_default_logger();

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
        match event {
            ComputeEvent::CodeProcessed(processed_code_id) => {
                assert_eq!(processed_code_id, code_id);
            }
            _ => panic!("Expected CodeProcessed event"),
        }
    }
}
