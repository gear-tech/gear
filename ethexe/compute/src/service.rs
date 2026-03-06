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

#[cfg(test)]
use crate::tests::MockProcessor;
use crate::{
    ComputeEvent, ProcessorExt, Result,
    codes::CodesSubService,
    compute::{ComputeConfig, ComputeSubService},
    prepare::PrepareSubService,
};
use ethexe_common::{Announce, CodeAndIdUnchecked, PromisePolicy};
use ethexe_db::Database;
use ethexe_processor::Processor;
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

pub struct ComputeService<P: ProcessorExt = Processor> {
    codes_sub_service: CodesSubService<P>,
    prepare_sub_service: PrepareSubService,
    compute_sub_service: ComputeSubService<P>,
}

impl<P: ProcessorExt> ComputeService<P> {
    /// Creates new compute service.
    pub fn new(config: ComputeConfig, db: Database, processor: P) -> Self {
        Self {
            prepare_sub_service: PrepareSubService::new(db.clone()),
            compute_sub_service: ComputeSubService::new(config, db.clone(), processor.clone()),
            codes_sub_service: CodesSubService::new(db, processor),
        }
    }
}

#[cfg(test)]
impl ComputeService {
    /// Creates the processor with default [`ComputeConfig::without_quarantine`] and [`Processor`] with default config.
    pub fn new_with_defaults(db: Database) -> Self {
        let config = ComputeConfig::without_quarantine();
        let processor = Processor::new(db.clone()).unwrap();
        Self::new(config, db, processor)
    }
}

#[cfg(test)]
impl ComputeService<MockProcessor> {
    pub fn new_mock_processor(db: Database) -> Self {
        Self::new(ComputeConfig::without_quarantine(), db, MockProcessor)
    }
}

impl<P: ProcessorExt> ComputeService<P> {
    // TODO #4550: consider to create Processor inside ComputeService

    pub fn process_code(&mut self, code_and_id: CodeAndIdUnchecked) {
        self.codes_sub_service.receive_code_to_process(code_and_id);
    }

    pub fn prepare_block(&mut self, block: H256) {
        self.prepare_sub_service.receive_block_to_prepare(block);
    }

    pub fn compute_announce(&mut self, announce: Announce, promise_policy: PromisePolicy) {
        self.compute_sub_service
            .receive_announce_to_compute(announce, promise_policy);
    }
}

impl<P: ProcessorExt> Stream for ComputeService<P> {
    type Item = Result<ComputeEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(result) = self.codes_sub_service.poll_next(cx) {
            match result {
                Ok(code_id) => {
                    self.prepare_sub_service.receive_processed_code(code_id);
                    return Poll::Ready(Some(Ok(ComputeEvent::CodeProcessed(code_id))));
                }
                Err(e) => {
                    return Poll::Ready(Some(Err(e)));
                }
            }
        };

        if let Poll::Ready(result) = self.prepare_sub_service.poll_next(cx) {
            return Poll::Ready(Some(result.map(ComputeEvent::from)));
        };

        if let Poll::Ready(event) = self.compute_sub_service.poll_next(cx) {
            return Poll::Ready(Some(event));
        };

        Poll::Pending
    }
}

impl<P: ProcessorExt> FusedStream for ComputeService<P> {
    fn is_terminated(&self) -> bool {
        false
    }
}

pub(crate) trait SubService: Unpin + Send + 'static {
    type Output;

    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Output>>;

    #[cfg(test)]
    async fn next(&mut self) -> Result<Self::Output> {
        futures::future::poll_fn(|cx| self.poll_next(cx)).await
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use ethexe_common::{CodeAndIdUnchecked, db::*, mock::*};
    use ethexe_db::Database as DB;
    use futures::StreamExt;
    use gear_core::ids::prelude::CodeIdExt;
    use gprimitives::CodeId;

    /// Test ComputeService block preparation functionality
    #[tokio::test]
    async fn prepare_block() {
        gear_utils::init_default_logger();

        let db = DB::memory();
        let mut service = ComputeService::new_mock_processor(db.clone());

        let chain = BlockChain::mock(1).setup(&db);
        let block = chain.blocks[1].to_simple().next_block().setup(&db);

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
        let mut service = ComputeService::new_mock_processor(db.clone());

        let chain = BlockChain::mock(1).setup(&db);

        let block = chain.blocks[1].to_simple().next_block().setup(&db);

        service.prepare_block(block.hash);
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(event, ComputeEvent::BlockPrepared(block.hash));

        // Request computation
        let announce = Announce {
            block_hash: block.hash,
            parent: chain.block_top_announce_hash(1),
            gas_allowance: Some(42),
            injected_transactions: vec![],
        };
        let announce_hash = announce.to_hash();
        service.compute_announce(announce, PromisePolicy::Disabled);

        // Poll service to process the block
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(event, ComputeEvent::AnnounceComputed(announce_hash));

        // Verify block is marked as computed in DB
        assert!(db.announce_meta(announce_hash).computed);
    }

    /// Test ComputeService code processing functionality
    #[tokio::test]
    async fn process_code() {
        gear_utils::init_default_logger();

        let db = DB::memory();
        let mut service = ComputeService::new_mock_processor(db.clone());

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
