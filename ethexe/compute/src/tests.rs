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
    CodeBlobInfo,
    db::*,
    events::{BlockEvent, RouterEvent},
    mock::*,
};
use ethexe_db::Database;
use ethexe_processor::Processor;
use futures::StreamExt;
use gear_core::ids::prelude::CodeIdExt;
use std::{cell::RefCell, collections::BTreeMap};

thread_local! {
    pub(crate) static PROCESSOR_RESULT: RefCell<BlockProcessingResult> = const { RefCell::new(
        BlockProcessingResult {
            transitions: Vec::new(),
            states: BTreeMap::new(),
            schedule: BTreeMap::new(),
        }
    ) };
}

// MockProcessor that implements ProcessorExt and always returns Ok with empty results
#[derive(Clone)]
pub(crate) struct MockProcessor;

impl ProcessorExt for MockProcessor {
    async fn process_announce(
        &mut self,
        _announce: Announce,
        _events: Vec<BlockRequestEvent>,
    ) -> Result<BlockProcessingResult> {
        let result = PROCESSOR_RESULT.with(|r| r.borrow().clone());
        PROCESSOR_RESULT.with(|r| {
            *r.borrow_mut() = BlockProcessingResult {
                transitions: vec![],
                states: BTreeMap::new(),
                schedule: BTreeMap::new(),
            }
        });

        Ok(result)
    }

    fn process_upload_code(&mut self, _code_and_id: CodeAndIdUnchecked) -> Result<bool> {
        Ok(true)
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

                BlockEvent::Router(RouterEvent::CodeGotValidated {
                    code_id,
                    valid: true,
                })
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

        let mut chain = BlockChain::mock(chain_len + 1);
        insert_code_events(&mut chain, events_in_block);
        mark_as_not_prepared(&mut chain);
        let chain = chain.setup(&db);

        let compute = ComputeService::new(db.clone(), Processor::new(db.clone()).unwrap());

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
        let announce_hash = announce.hash();
        self.compute.compute_announce(announce);

        let event = self
            .compute
            .next()
            .await
            .unwrap()
            .expect("expect block will be processing");

        let processed_announce = event.unwrap_announce_computed();
        assert_eq!(processed_announce, announce_hash);
    }
}

fn new_announce(db: &Database, block_hash: H256, gas_allowance: Option<u64>) -> Announce {
    let parent_hash = db.block_header(block_hash).unwrap().parent_hash;
    let parent_announce_hash = db.top_announce_hash(parent_hash);
    Announce {
        block_hash,
        parent: parent_announce_hash,
        gas_allowance,
        off_chain_transactions: vec![],
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
    block_events.push(BlockEvent::Router(RouterEvent::CodeValidationRequested {
        code_id: CodeId::zero(),
        timestamp: 0u64,
        tx_hash: H256::random(),
    }));
    env.db
        .set_block_events(env.chain.blocks[1].hash, &block_events);
    env.prepare_and_assert_block(env.chain.blocks[1].hash).await;

    let announce = new_announce(&env.db, env.chain.blocks[1].hash, Some(100));
    env.compute_and_assert_announce(announce.clone()).await;
    env.compute_and_assert_announce(announce.clone()).await;

    Ok(())
}
