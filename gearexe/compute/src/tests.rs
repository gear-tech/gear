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
use gearexe_common::{
    Address, BlockHeader, CodeAndIdUnchecked, Digest,
    db::{BlockMetaStorageWrite, OnChainStorageRead, OnChainStorageWrite},
    events::{BlockEvent, RouterEvent},
};
use gearexe_db::Database;
use gearexe_processor::Processor;
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
    async fn process_block_events(
        &mut self,
        _block: H256,
        _events: Vec<BlockRequestEvent>,
    ) -> Result<BlockProcessingResult> {
        let result = PROCESSOR_RESULT.with_borrow(|r| r.clone());
        PROCESSOR_RESULT.with_borrow_mut(|r| {
            *r = BlockProcessingResult {
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
        meta.last_committed_batch = Some(Digest::random());
        meta.last_committed_head = Some(H256::random());
    });
    db.set_block_outcome(genesis_hash, vec![]);
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
    db.set_validators(genesis_hash, nonempty![Address::from([0u8; 20])]);

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
