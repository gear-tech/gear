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
    use crate::tests::{
        MockProcessor, block_chain_strategy, next_compute_event, proptest_config, run_async_test,
    };
    use ethexe_common::{
        BlockHeader, CodeAndIdUnchecked,
        db::{
            BlockMetaStorageRO, CodesStorageRO, CompactMb, MbStorageRO, MbStorageRW,
            OnChainStorageRW,
        },
        malachite::{ProcessQueuesLimits, ProgressTasksLimits, Transaction, Transactions},
        mock::Tap,
    };
    use ethexe_db::Database as DB;
    use gear_core::ids::prelude::CodeIdExt;
    use gprimitives::{CodeId, H256};
    use proptest::{collection, prelude::*};

    fn seed_mb(db: &DB, mb_hash: H256, gas_allowance: u64) {
        let eth_block_hash = H256::from_low_u64_be(0xEB00);
        db.set_block_header(
            eth_block_hash,
            BlockHeader {
                height: 1,
                timestamp: 1,
                parent_hash: H256::zero(),
            },
        );
        db.set_block_events(eth_block_hash, &[]);

        let transactions_hash = db.set_transactions(Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                block_hash: eth_block_hash,
            },
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits { gas_allowance },
            },
        ]));

        db.set_mb_compact_block(
            mb_hash,
            CompactMb {
                parent: H256::zero(),
                height: 1,
                transactions_hash,
            },
        );
    }

    proptest! {
        #![proptest_config(proptest_config(64))]

        #[test]
        fn prepare_block(chain in block_chain_strategy(1)) {
            gear_utils::init_default_logger();

            run_async_test(async move {
                let db = DB::memory();
                let mut service = ComputeService::new_mock_processor(db.clone());

                let chain = chain.setup(&db);
                let block = chain.blocks[1].to_simple().next_block().setup(&db);

                service.prepare_block(block.hash);

                let event = next_compute_event(&mut service).await;
                assert_eq!(event, ComputeEvent::BlockPrepared(block.hash));
                assert!(db.block_meta(block.hash).prepared);
            });
        }

        #[test]
        fn compute_mb(gas_allowance in 1u64..=1_000_000) {
            gear_utils::init_default_logger();

            run_async_test(async move {
                let db = DB::memory();
                let mut service = ComputeService::new_mock_processor(db.clone());
                let mb_hash = H256::from_low_u64_be(0xCAFE);

                seed_mb(&db, mb_hash, gas_allowance);
                service.compute_mb(mb_hash, PromisePolicy::Disabled);

                assert_eq!(
                    next_compute_event(&mut service).await,
                    ComputeEvent::MbComputed(mb_hash)
                );
                assert!(db.mb_meta(mb_hash).computed);
            });
        }

        #[test]
        fn process_code(code in collection::vec(any::<u8>(), 1..=64)) {
            gear_utils::init_default_logger();

            run_async_test(async move {
                let code_id = CodeId::generate(&code);
                let db = DB::memory();
                let processor = MockProcessor::with_default_valid_code()
                    .tap_mut(|p| p.process_codes_result.as_mut().unwrap().code_id = code_id);
                let mut service = ComputeService::new(db.clone(), processor.clone());

                assert!(db.code_valid(code_id).is_none());

                service.process_code(CodeAndIdUnchecked { code, code_id });

                assert_eq!(
                    next_compute_event(&mut service).await,
                    ComputeEvent::CodeProcessed(code_id)
                );
                assert_eq!(processor.process_code_call_count(), 1);
                assert_eq!(db.code_valid(code_id), Some(true));
            });
        }
    }
}
