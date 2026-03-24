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

use super::*;
use ethexe_common::{
    CodeBlobInfo, PromisePolicy,
    db::*,
    events::{
        BlockEvent, RouterEvent,
        router::{CodeGotValidatedEvent, CodeValidationRequestedEvent},
    },
    mock::*,
};
use ethexe_db::Database;
use ethexe_processor::ValidCodeInfo;
use futures::StreamExt;
use gear_core::{
    code::{CodeMetadata, InstantiatedSectionSizes, InstrumentedCode},
    ids::prelude::CodeIdExt,
};
use std::time::Duration;
use tokio::{sync::mpsc, time::timeout};

// MockProcessor that implements ProcessorExt and always returns Ok with empty results
#[derive(Clone, Default)]
pub(crate) struct MockProcessor {
    pub process_programs_result: Option<FinalizedBlockTransitions>,
    pub process_codes_result: Option<ProcessedCodeInfo>,
    pub process_code_calls: std::sync::Arc<tokio::sync::Mutex<Vec<CodeAndIdUnchecked>>>,
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
            process_code_calls: std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    pub async fn process_code_call_count(&self) -> usize {
        self.process_code_calls.lock().await.len()
    }

}

impl ProcessorExt for MockProcessor {
    async fn process_programs(
        &mut self,
        _executable: ExecutableData,
        _promise_out_tx: Option<mpsc::UnboundedSender<Promise>>,
    ) -> Result<FinalizedBlockTransitions> {
        Ok(self.process_programs_result.take().unwrap_or_default())
    }

    fn process_code(&mut self, code_and_id: CodeAndIdUnchecked) -> Result<ProcessedCodeInfo> {
        let mut calls = futures::executor::block_on(self.process_code_calls.lock());
        calls.push(code_and_id.clone());
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

// Generate codes for the given chain and store the events in the database
// Return a map with `CodeId` and corresponding code bytes
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

    // remove all announces except genesis announce
    let genesis_announce_hash = chain.block_top_announce_hash(0);
    chain
        .announces
        .retain(|hash, _| *hash == genesis_announce_hash);
}

struct TestEnv {
    db: Database,
    compute: ComputeService,
    chain: BlockChain,
}

impl TestEnv {
    // Setup the chain and compute service.
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

        let event = self
            .compute
            .next()
            .await
            .unwrap()
            .expect("expect compute service request codes to load");
        let codes_to_load = event.unwrap_request_load_codes();

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

            let event = self
                .compute
                .next()
                .await
                .unwrap()
                .expect("expect code will be processing");
            let processed_code_id = event.unwrap_code_processed();

            assert_eq!(processed_code_id, code_id);
        }

        let event = self
            .compute
            .next()
            .await
            .unwrap()
            .expect("expect block prepared after processing all codes");
        let prepared_block = event.unwrap_block_prepared();
        assert_eq!(prepared_block, block);
    }

    async fn compute_and_assert_announce(&mut self, announce: Announce) {
        let announce_hash = announce.to_hash();
        self.compute
            .compute_announce(announce.clone(), PromisePolicy::Disabled);

        let event = self
            .compute
            .next()
            .await
            .unwrap()
            .expect("expect block will be processing");

        let computed_announce = event.unwrap_announce_computed();
        assert_eq!(computed_announce, announce_hash);

        self.db.mutate_block_meta(announce.block_hash, |meta| {
            meta.announces.get_or_insert_default().insert(announce_hash);
        });
    }
}

#[track_caller]
fn new_announce(db: &Database, block_hash: H256, gas_allowance: Option<u64>) -> Announce {
    let parent_hash = db.block_header(block_hash).unwrap().parent_hash;
    let parent_announce_hash = db.top_announce_hash(parent_hash);
    Announce {
        block_hash,
        parent: parent_announce_hash,
        gas_allowance,
        injected_transactions: vec![],
    }
}

#[tokio::test]
async fn block_computation_basic() -> Result<()> {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(1, 3);

    for block in env.chain.blocks.clone().iter().skip(1) {
        env.prepare_and_assert_block(block.hash).await;

        let announce = new_announce(&env.db, block.hash, Some(100));
        env.compute_and_assert_announce(announce).await;
    }

    Ok(())
}

#[tokio::test]
async fn multiple_preparation_and_one_processing() -> Result<()> {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(3, 3);

    for block in env.chain.blocks.clone().iter().skip(1) {
        env.prepare_and_assert_block(block.hash).await;
    }

    // append announces to prepared blocks, except the last one, so that it can be computed
    for i in 1..3 {
        let announce = new_announce(&env.db, env.chain.blocks[i].hash, Some(100));
        env.db.mutate_block_meta(announce.block_hash, |meta| {
            meta.announces
                .get_or_insert_default()
                .insert(announce.to_hash());
        });
        env.db.set_announce(announce);
    }

    let announce = new_announce(&env.db, env.chain.blocks[3].hash, Some(100));
    env.compute_and_assert_announce(announce).await;

    Ok(())
}

#[tokio::test]
async fn one_preparation_and_multiple_processing() -> Result<()> {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(3, 3);

    env.prepare_and_assert_block(env.chain.blocks[3].hash).await;

    for block in env.chain.blocks.clone().iter().skip(1) {
        let announce = new_announce(&env.db, block.hash, Some(100));
        env.compute_and_assert_announce(announce).await;
    }

    Ok(())
}

#[tokio::test]
async fn code_validation_request_does_not_block_preparation() -> Result<()> {
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(1, 3);

    let mut block_events = env.chain.blocks[1].as_synced().events.clone();

    // add invalid event which shouldn't stop block prepare
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

    let announce = new_announce(&env.db, env.chain.blocks[1].hash, Some(100));
    env.compute_and_assert_announce(announce.clone()).await;
    env.compute_and_assert_announce(announce.clone()).await;

    Ok(())
}

#[tokio::test]
async fn code_validation_request_for_already_processed_code_does_not_request_loading() -> Result<()>
{
    gear_utils::init_default_logger();

    let mut env = TestEnv::new(1, 0);
    let block_hash = env.chain.blocks[1].hash;

    let code = create_new_code(1);
    let code_id = CodeId::generate(&code);
    env.db.set_original_code(&code);
    env.db.set_code_valid(code_id, true);

    env.db.set_block_events(
        block_hash,
        &[BlockEvent::Router(RouterEvent::CodeValidationRequested(
            CodeValidationRequestedEvent {
                code_id,
                timestamp: 0u64,
                tx_hash: H256::random(),
            },
        ))],
    );

    env.compute.prepare_block(block_hash);

    let event = env
        .compute
        .next()
        .await
        .unwrap()
        .expect("expect block prepared without requesting code loading");
    let prepared_block = event.unwrap_block_prepared();
    assert_eq!(prepared_block, block_hash);

    let no_follow_up_event = timeout(Duration::from_millis(100), env.compute.next()).await;
    assert!(
        no_follow_up_event.is_err(),
        "unexpected follow-up compute event after block preparation: {no_follow_up_event:?}",
    );

    Ok(())
}

#[tokio::test]
async fn process_code_for_already_processed_valid_code_emits_code_processed() -> Result<()> {
    gear_utils::init_default_logger();

    let db = Database::memory();
    let processor = MockProcessor::default();
    let mut compute = ComputeService::new(
        ComputeConfig::without_quarantine(),
        db.clone(),
        processor.clone(),
    );

    let code = create_new_code(2);
    let code_id = CodeId::generate(&code);

    db.set_original_code(&code);
    db.set_instrumented_code(
        ethexe_runtime_common::VERSION,
        code_id,
        InstrumentedCode::new(vec![0], InstantiatedSectionSizes::new(0, 0, 0, 0, 0, 0)),
    );
    db.set_code_valid(code_id, true);

    compute.process_code(CodeAndIdUnchecked { code_id, code });

    let event = compute
        .next()
        .await
        .unwrap()
        .expect("expect already processed code to produce CodeProcessed event");
    let processed_code_id = event.unwrap_code_processed();
    assert_eq!(processed_code_id, code_id);

    // Verify that the processor was NOT called for already-validated code
    // The CodesSubService should short-circuit and emit CodeProcessed without calling the processor
    assert_eq!(
        processor.process_code_call_count().await,
        0,
        "Processor should not be called for already-validated code"
    );

    Ok(())
}
