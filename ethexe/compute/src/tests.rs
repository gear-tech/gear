use super::*;
use ethexe_common::{
    db::{BlockMetaStorageWrite, OnChainStorageRead, OnChainStorageWrite},
    events::{BlockEvent, RouterEvent},
    BlockHeader, CodeAndIdUnchecked, Digest,
};
use ethexe_db::Database;
use ethexe_processor::Processor;
use futures::StreamExt;
use gear_core::ids::prelude::CodeIdExt;
use std::collections::{HashMap, VecDeque};

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
    });
    db.set_block_outcome(genesis_hash, vec![]);
    db.set_previous_not_empty_block(genesis_hash, H256::random());
    db.set_last_committed_batch(genesis_hash, Digest::random());
    db.set_block_commitment_queue(genesis_hash, Default::default());
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

// #[tokio::test]
// async fn test_codes_queue_propagation() {
//     let db = Database::memory();

//     // Prepare test data
//     let parent_block = H256::random();
//     let current_block = H256::random();
//     let code_id_1 = H256::random().into();
//     let code_id_2 = H256::random().into();

//     // Simulate parent block with a codes queue
//     let mut parent_codes_queue = VecDeque::new();
//     parent_codes_queue.push_back(code_id_1);
//     db.set_block_codes_queue(parent_block, parent_codes_queue.clone());
//     db.set_block_outcome(parent_block, Default::default());
//     db.set_previous_not_empty_block(parent_block, H256::random());
//     db.set_last_committed_batch(parent_block, Digest::random());
//     db.set_block_commitment_queue(parent_block, Default::default());

//     // Simulate events for the current block
//     let events = vec![
//         BlockEvent::Router(RouterEvent::CodeGotValidated {
//             code_id: code_id_1,
//             valid: true,
//         }),
//         BlockEvent::Router(RouterEvent::CodeValidationRequested {
//             code_id: code_id_2,
//             timestamp: 0,
//             tx_hash: H256::random(),
//         }),
//     ];
//     db.set_block_events(current_block, &events);

//     // Propagate data from parent
//     ChainHeadProcessContext::propagate_data_from_parent(
//         &db,
//         current_block,
//         parent_block,
//         db.block_events(current_block).unwrap().iter(),
//     )
//     .unwrap();

//     // Check for parent
//     let codes_queue = db.block_codes_queue(parent_block).unwrap();
//     assert_eq!(codes_queue, parent_codes_queue);

//     // Check for current block
//     let codes_queue = db.block_codes_queue(current_block).unwrap();
//     assert_eq!(codes_queue, VecDeque::from(vec![code_id_2]));
// }

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
