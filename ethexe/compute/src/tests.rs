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
    Address, BlockHeader, SimpleBlockData,
    db::*,
    events::{BlockEvent, RouterEvent},
    setup_genesis_in_db,
};
use ethexe_db::Database;
use ethexe_processor::Processor;
use futures::StreamExt;
use gear_core::ids::prelude::CodeIdExt;
use nonempty::nonempty;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, VecDeque},
};

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
fn generate_codes(
    db: &impl OnChainStorageWrite,
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

        log::trace!("Generated {:?} events for block {}", events, block);
        db.set_block_events(block, &events);
    }
    codes_storage
}

// Generate a chain with the given length and setup the genesis block
fn generate_chain(db: &Database, chain_len: u32) -> VecDeque<H256> {
    let mut chain = VecDeque::new();

    let mut parent_hash = H256::from_low_u64_be(u64::MAX);
    for block_num in 0..chain_len + 1 {
        let block = SimpleBlockData {
            hash: H256::from_low_u64_be(
                // Use block number as low 32 bits of the hash, avoiding zero hash
                if block_num != 0 { block_num } else { u32::MAX } as u64,
            ),
            header: BlockHeader {
                height: block_num,
                timestamp: (block_num * 10) as u64,
                parent_hash,
            },
        };

        if block_num == 0 {
            setup_genesis_in_db(db, block.clone(), nonempty![Address::from([1; 20])]);
        }

        db.set_block_header(block.hash, block.header);
        db.set_block_events(block.hash, Default::default());
        db.set_block_synced(block.hash);

        chain.push_back(block.hash);
        parent_hash = block.hash;
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

    async fn compute_and_assert_announce(&mut self, announce: Announce) {
        let announce_hash = announce.hash();
        self.inner.compute_announce(announce);

        let event = self
            .inner
            .next()
            .await
            .unwrap()
            .expect("expect block will be processing");

        let processed_announce = event.unwrap_announce_computed();
        assert_eq!(processed_announce, announce_hash);
    }
}

// Setup the chain and compute service.
// It is needed to reduce the copy-paste in tests.
fn setup(
    db: &Database,
    chain_len: u32,
    events_in_block: u32,
) -> (VecDeque<H256>, WrappedComputeService) {
    let chain = generate_chain(db, chain_len);
    let codes_storage = generate_codes(db, &chain, events_in_block);

    let compute = WrappedComputeService {
        inner: ComputeService::new(db.clone(), Processor::new(db.clone()).unwrap()),
        codes_storage,
    };
    (chain, compute)
}

fn new_announce(db: &Database, block_hash: H256, gas_allowance: Option<u64>) -> Announce {
    let parent_hash = db.block_header(block_hash).unwrap().parent_hash;
    let parent_announce_hash = db.block_meta(parent_hash).announces.unwrap()[0];
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

    let chain_len = 1;
    let db = Database::memory();
    let (chain, mut compute) = setup(&db, chain_len, 3);

    for block in chain.into_iter().skip(1) {
        compute.prepare_and_assert_block(block).await;

        let announce = new_announce(&db, block, Some(100));
        compute.compute_and_assert_announce(announce).await;
    }

    Ok(())
}

#[tokio::test]
async fn multiple_preparation_and_one_processing() -> Result<()> {
    gear_utils::init_default_logger();

    let chain_len = 3;
    let db = Database::memory();
    let (mut chain, mut compute) = setup(&db, chain_len, 3);

    let _genesis = chain.pop_front().unwrap();
    let block1 = chain.pop_front().unwrap();
    let block2 = chain.pop_front().unwrap();
    let block3 = chain.pop_front().unwrap();

    compute.prepare_and_assert_block(block1).await;
    compute.prepare_and_assert_block(block2).await;
    compute.prepare_and_assert_block(block3).await;

    let announce = new_announce(&db, block3, Some(100));
    compute.compute_and_assert_announce(announce).await;

    Ok(())
}

#[tokio::test]
async fn one_preparation_and_multiple_processing() -> Result<()> {
    gear_utils::init_default_logger();

    let chain_len = 3;
    let db = Database::memory();
    let (mut chain, mut compute) = setup(&db, chain_len, 3);

    let _genesis = chain.pop_front().unwrap();
    let block1 = chain.pop_front().unwrap();
    let block2 = chain.pop_front().unwrap();
    let block3 = chain.pop_front().unwrap();

    log::trace!("blocks: {_genesis}, {}, {}, {}", block1, block2, block3);

    compute.prepare_and_assert_block(block3).await;

    let announce1 = new_announce(&db, block1, Some(100));
    compute.compute_and_assert_announce(announce1).await;

    let announce2 = new_announce(&db, block2, Some(100));
    compute.compute_and_assert_announce(announce2).await;

    let announce3 = new_announce(&db, block3, Some(100));
    compute.compute_and_assert_announce(announce3).await;

    Ok(())
}

#[tokio::test]
async fn code_validation_request_does_not_block_preparation() -> Result<()> {
    gear_utils::init_default_logger();

    let chain_len = 1;
    let db = Database::memory();
    let (mut chain, mut compute) = setup(&db, chain_len, 3);

    let block = chain.pop_back().unwrap();
    let mut block_events = db.block_events(block).unwrap();

    // add invalid event which shouldn't stop block prepare
    block_events.push(BlockEvent::Router(RouterEvent::CodeValidationRequested {
        code_id: CodeId::zero(),
        timestamp: 0u64,
        tx_hash: H256::random(),
    }));
    db.set_block_events(block, &block_events);
    compute.prepare_and_assert_block(block).await;

    let announce = new_announce(&db, block, Some(100));
    compute.compute_and_assert_announce(announce.clone()).await;
    compute.compute_and_assert_announce(announce.clone()).await;

    Ok(())
}
