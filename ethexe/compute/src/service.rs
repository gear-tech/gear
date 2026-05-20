// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(test)]
use crate::tests::MockProcessor;
use crate::{
    ComputeEvent, ProcessorExt, Result, codes::CodesSubService, compute::ComputeSubService,
    prepare::PrepareSubService,
};
use ethexe_common::{CodeAndIdUnchecked, PromiseEmissionMode, PromisePolicy};
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
    mb_compute_sub_service: ComputeSubService<P>,
}

impl<P: ProcessorExt> ComputeService<P> {
    /// Creates new compute service. Promises follow the consensus
    /// layer's per-MB decision; use [`Self::with_promise_mode`] to
    /// override.
    pub fn new(db: Database, processor: P) -> Self {
        Self::with_promise_mode(db, processor, PromiseEmissionMode::default())
    }

    /// Creates a compute service with an explicit promise emission mode.
    /// The mode is forwarded to the MB sub-service so predecessor MBs
    /// emit promises too under `AlwaysEmit`.
    pub fn with_promise_mode(
        db: Database,
        processor: P,
        promise_emission_mode: PromiseEmissionMode,
    ) -> Self {
        Self {
            prepare_sub_service: PrepareSubService::new(db.clone()),
            mb_compute_sub_service: ComputeSubService::with_promise_mode(
                db.clone(),
                processor.clone(),
                promise_emission_mode,
            ),
            codes_sub_service: CodesSubService::new(db, processor),
        }
    }
}

#[cfg(test)]
impl ComputeService {
    /// Builds a [`ComputeService`] with a default [`Processor`].
    pub fn new_with_defaults(db: Database) -> Self {
        let processor = Processor::new(db.clone()).unwrap();
        Self::new(db, processor)
    }
}

#[cfg(test)]
impl ComputeService<MockProcessor> {
    pub fn new_mock_processor(db: Database) -> Self {
        Self::new(db, MockProcessor::default())
    }
}

impl<P: ProcessorExt> ComputeService<P> {
    pub fn process_code(&mut self, code_and_id: CodeAndIdUnchecked) {
        self.codes_sub_service.receive_code_to_process(code_and_id);
    }

    pub fn prepare_block(&mut self, block: H256) {
        self.prepare_sub_service.receive_block_to_prepare(block);
    }

    pub fn compute_mb(&mut self, mb_hash: H256, policy: PromisePolicy) {
        self.mb_compute_sub_service.receive_mb(mb_hash, policy);
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

        if let Poll::Ready(event) = self.mb_compute_sub_service.poll_next(cx) {
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

    /// Test ComputeService code processing functionality
    #[tokio::test]
    async fn process_code() {
        gear_utils::init_default_logger();

        let code = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]; // Simple WASM header
        let code_id = CodeId::generate(&code);

        let db = DB::memory();
        let processor = MockProcessor::with_default_valid_code()
            .tap_mut(|p| p.process_codes_result.as_mut().unwrap().code_id = code_id);
        let mut service = ComputeService::new(db.clone(), processor.clone());

        // Create test code

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

        // Verify that the processor was called for non-validated code
        assert_eq!(
            processor.process_code_call_count(),
            1,
            "Processor should be called for non-validated code"
        );

        // Verify code is now marked as valid in DB
        assert_eq!(db.code_valid(code_id), Some(true));
    }
}
