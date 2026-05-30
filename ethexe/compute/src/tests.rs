// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::*;
use crate::service::SubService;
use ethexe_common::{
    CodeBlobInfo,
    db::*,
    events::{
        BlockEvent, RouterEvent,
        router::{CodeGotValidatedEvent, CodeValidationRequestedEvent},
    },
    mock::*,
};
use ethexe_db::Database;
use ethexe_processor::{BoundPromiseSink, ValidCodeInfo};
use futures::{Future, StreamExt};
use gear_core::{
    code::{CodeMetadata, InstantiatedSectionSizes, InstrumentedCode},
    ids::prelude::CodeIdExt,
};
use gprimitives::{CodeId, H256};
use proptest::{collection, prelude::*};
use std::time::Duration;
use tokio::{runtime::Builder, time::timeout};

thread_local! {
    // Reuse one current-thread runtime per test thread to avoid rebuilding it for every proptest case.
    static TEST_RUNTIME: tokio::runtime::Runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
}

pub(crate) const ASYNC_EVENT_TIMEOUT: Duration = Duration::from_secs(3);
const NO_EVENT_TIMEOUT: Duration = Duration::from_millis(500);
const PROPTEST_TIMEOUT_MS: u32 = 60_000;

pub(crate) fn block_chain_strategy(len: u32) -> BoxedStrategy<BlockChain> {
    any_with::<BlockChain>(BlockChainParams::from(len)).boxed()
}

pub(crate) fn distinct_code_ids_sorted(count: usize) -> BoxedStrategy<Vec<CodeId>> {
    collection::btree_set(any::<[u8; 32]>().prop_map(CodeId::from), count)
        .prop_map(|ids| ids.into_iter().collect())
        .boxed()
}

pub(crate) fn run_async_test<F: Future>(future: F) -> F::Output {
    TEST_RUNTIME.with(|runtime| runtime.block_on(future))
}

pub(crate) async fn next_compute_event<P: ProcessorExt>(
    compute: &mut ComputeService<P>,
) -> ComputeEvent {
    timeout(ASYNC_EVENT_TIMEOUT, compute.next())
        .await
        .expect("timed out waiting for compute event")
        .expect("compute stream ended")
        .expect("compute service returned error")
}

pub(crate) async fn next_subservice_event<S: SubService>(service: &mut S) -> S::Output {
    timeout(ASYNC_EVENT_TIMEOUT, service.next())
        .await
        .expect("timed out waiting for sub-service event")
        .expect("sub-service returned error")
}

pub(crate) async fn assert_no_compute_event<P: ProcessorExt>(compute: &mut ComputeService<P>) {
    assert!(
        timeout(NO_EVENT_TIMEOUT, compute.next()).await.is_err(),
        "unexpected follow-up compute event"
    );
}

pub(crate) fn proptest_config(cases: u32) -> ProptestConfig {
    ProptestConfig {
        cases,
        timeout: PROPTEST_TIMEOUT_MS,
        ..ProptestConfig::default()
    }
}

// MockProcessor that implements ProcessorExt and always returns Ok with empty results
#[derive(Clone, Default)]
pub(crate) struct MockProcessor {
    pub process_programs_result: Option<FinalizedBlockTransitions>,
    pub process_codes_result: Option<ProcessedCodeInfo>,
    pub process_code_calls: std::sync::Arc<std::sync::Mutex<Vec<CodeAndIdUnchecked>>>,
}

impl MockProcessor {
    pub fn with_default_valid_code() -> Self {
        Self {
            process_programs_result: None,
            process_codes_result: Some(ProcessedCodeInfo {
                code_id: CodeId::zero(),
                valid: Some(ValidCodeInfo {
                    code: vec![],
                    instrumented_code: InstrumentedCode::new(
                        vec![],
                        InstantiatedSectionSizes::new(0, 0, 0, 0, 0, 0),
                    ),
                    code_metadata: CodeMetadata::new(
                        0,
                        Default::default(),
                        0.into(),
                        None,
                        gear_core::code::InstrumentationStatus::Instrumented {
                            version: 0,
                            code_len: 0,
                        },
                    ),
                }),
            }),
            process_code_calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn process_code_call_count(&self) -> usize {
        self.process_code_calls.lock().unwrap().len()
    }
}

impl ProcessorExt for MockProcessor {
    async fn process_programs(
        &mut self,
        _executable: ethexe_processor::ExecutableData,
        _promise_sink: Option<BoundPromiseSink>,
    ) -> Result<FinalizedBlockTransitions> {
        Ok(self.process_programs_result.take().unwrap_or_default())
    }

    async fn process_code(&mut self, code_and_id: CodeAndIdUnchecked) -> Result<ProcessedCodeInfo> {
        self.process_code_calls
            .lock()
            .unwrap()
            .push(code_and_id.clone());
        Ok(self
            .process_codes_result
            .take()
            .unwrap_or(ProcessedCodeInfo {
                code_id: code_and_id.code_id,
                valid: None,
            }))
    }
}

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

// Generate codes for the given chain and store the events in the database.
fn insert_code_events(chain: &mut BlockChain, events_in_block: u32) {
    let mut nonce = 0;
    for data in chain.blocks.iter_mut().map(|data| data.as_synced_mut()) {
        data.events = (0..events_in_block)
            .map(|_| {
                nonce += 1;
                let code = create_new_code(nonce);
                let code_id = CodeId::generate(&code);
                chain.codes.insert(
                    code_id,
                    CodeData {
                        original_bytes: code,
                        blob_info: CodeBlobInfo::default(),
                        instrumented: None,
                    },
                );

                BlockEvent::Router(RouterEvent::CodeGotValidated(CodeGotValidatedEvent {
                    code_id,
                    valid: true,
                }))
            })
            .collect();
    }
}

fn mark_as_not_prepared(chain: &mut BlockChain) {
    // skip genesis
    for block in chain.blocks.iter_mut().skip(1) {
        block.prepared = None;
    }
}

struct TestEnv {
    db: Database,
    compute: ComputeService,
    chain: BlockChain,
}

impl TestEnv {
    fn new(chain_len: u32, events_in_block: u32) -> TestEnv {
        let db = Database::memory();

        let mut chain = BlockChain::mock(chain_len);
        insert_code_events(&mut chain, events_in_block);
        mark_as_not_prepared(&mut chain);
        chain = chain.setup(&db);

        let compute = ComputeService::new_with_defaults(db.clone());

        TestEnv { db, compute, chain }
    }

    async fn prepare_and_assert_block(&mut self, block: H256) {
        self.compute.prepare_block(block);

        match next_compute_event(&mut self.compute).await {
            ComputeEvent::RequestLoadCodes(codes_to_load) => {
                for code_id in codes_to_load {
                    let Some(CodeData {
                        original_bytes: code,
                        ..
                    }) = self.chain.codes.remove(&code_id)
                    else {
                        continue;
                    };

                    self.compute
                        .process_code(CodeAndIdUnchecked { code, code_id });

                    let processed_code_id = next_compute_event(&mut self.compute)
                        .await
                        .unwrap_code_processed();
                    assert_eq!(processed_code_id, code_id);
                }

                let prepared_block = next_compute_event(&mut self.compute)
                    .await
                    .unwrap_block_prepared();
                assert_eq!(prepared_block, block);
            }
            ComputeEvent::BlockPrepared(prepared_block) => {
                assert_eq!(prepared_block, block);
            }
            event => panic!("unexpected compute event while preparing block: {event:?}"),
        }
    }
}

#[tokio::test]
async fn block_computation_basic() -> Result<()> {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(1, 3);

    for block in env.chain.blocks.clone().iter().skip(1) {
        env.prepare_and_assert_block(block.hash).await;
    }

    Ok(())
}

#[tokio::test]
async fn code_validation_request_does_not_block_preparation() -> Result<()> {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(1, 3);

    let mut block_events = env.chain.blocks[1].as_synced().events.clone();
    block_events.push(BlockEvent::Router(RouterEvent::CodeValidationRequested(
        CodeValidationRequestedEvent {
            code_id: CodeId::zero(),
            timestamp: 0u64,
            tx_hash: H256::random(),
        },
    )));

    env.db
        .set_block_events(env.chain.blocks[1].hash, &block_events);
    env.prepare_and_assert_block(env.chain.blocks[1].hash).await;

    Ok(())
}

#[tokio::test]
async fn code_validation_request_for_already_processed_code_does_not_request_loading() -> Result<()>
{
    gear_utils::init_default_logger();

    let db = Database::memory();
    let processor = MockProcessor::default();
    let mut compute = ComputeService::new(db.clone(), processor.clone());

    let code = create_new_code(1);
    let code_id = db.set_original_code(&code);
    db.set_code_valid(code_id, true);

    let mut chain = BlockChain::mock(1);
    mark_as_not_prepared(&mut chain);
    let chain = chain.setup(&db);
    let block_hash = chain.blocks[1].hash;

    let mut new_events = db.block_events(block_hash).unwrap_or_default();
    new_events.push(BlockEvent::Router(RouterEvent::CodeValidationRequested(
        CodeValidationRequestedEvent {
            code_id,
            timestamp: 0u64,
            tx_hash: H256::random(),
        },
    )));
    db.set_block_events(block_hash, &new_events);

    compute.prepare_block(block_hash);

    let prepared_block = next_compute_event(&mut compute)
        .await
        .unwrap_block_prepared();
    assert_eq!(prepared_block, block_hash);
    assert_no_compute_event(&mut compute).await;
    assert_eq!(processor.process_code_call_count(), 0);

    Ok(())
}

#[tokio::test]
async fn code_validation_request_for_non_validated_code_requests_loading() -> Result<()> {
    gear_utils::init_default_logger();

    let db = Database::memory();
    let processor = MockProcessor::default();
    let mut compute = ComputeService::new(db.clone(), processor.clone());

    let code = create_new_code(1);
    let code_id = db.set_original_code(&code);

    let mut chain = BlockChain::mock(1);
    mark_as_not_prepared(&mut chain);
    let chain = chain.setup(&db);
    let block_hash = chain.blocks[1].hash;

    let mut new_events = db.block_events(block_hash).unwrap_or_default();
    new_events.push(BlockEvent::Router(RouterEvent::CodeValidationRequested(
        CodeValidationRequestedEvent {
            code_id,
            timestamp: 0u64,
            tx_hash: H256::random(),
        },
    )));
    db.set_block_events(block_hash, &new_events);

    compute.prepare_block(block_hash);

    let codes_to_load = next_compute_event(&mut compute)
        .await
        .unwrap_request_load_codes();
    assert!(codes_to_load.contains(&code_id));

    Ok(())
}

#[tokio::test]
async fn process_code_for_already_processed_valid_code_emits_code_processed() -> Result<()> {
    gear_utils::init_default_logger();

    let db = Database::memory();
    let processor = MockProcessor::default();
    let mut compute = ComputeService::new(db.clone(), processor.clone());

    let code = create_new_code(2);
    let code_id = db.set_original_code(&code);

    db.set_instrumented_code(
        ethexe_runtime_common::VERSION,
        code_id,
        InstrumentedCode::new(vec![0], InstantiatedSectionSizes::new(0, 0, 0, 0, 0, 0)),
    );
    db.set_code_valid(code_id, true);

    compute.process_code(CodeAndIdUnchecked { code_id, code });

    let processed_code_id = next_compute_event(&mut compute)
        .await
        .unwrap_code_processed();
    assert_eq!(processed_code_id, code_id);
    assert_eq!(processor.process_code_call_count(), 0);

    Ok(())
}
