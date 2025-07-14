// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

// Modified implementation of the basic block-authorship logic from
// https://github.com/paritytech/substrate/tree/master/client/basic-authorship.
// The block proposer explicitly pushes the `pallet_gear::run`
// extrinsic at the end of each block.

#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

use crate::{
    ProposerFactory,
    authorship::MAX_SKIPPED_TRANSACTIONS,
    block_builder::{BlockBuilder, BlockBuilderBuilder},
};
use core::convert::TryFrom;
use demo_constructor::{Calls, Scheme, WASM_BINARY};
use frame_support::{assert_ok, storage::storage_prefix, traits::PalletInfoAccess};
use futures::executor::block_on;
use gear_core::program::Program;
use pallet_gear_rpc_runtime_api::GearApi;
use parity_scale_codec::{Decode, Encode};
use parking_lot::Mutex;
use runtime_primitives::{Block as TestBlock, BlockNumber};
use sc_client_api::Backend as _;
use sc_service::client::Client;
use sc_transaction_pool::{BasicPool, FullPool};
use sc_transaction_pool_api::{
    ChainEvent, MaintainedTransactionPool, TransactionPool, TransactionSource,
};
use sp_api::{ApiExt, Core, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_consensus::{BlockOrigin, Environment, Proposer};
use sp_consensus_babe::{
    BABE_ENGINE_ID, Slot,
    digests::{PreDigest, SecondaryPlainPreDigest},
};
use sp_inherents::InherentDataProvider;
use sp_runtime::{
    Digest, DigestItem, OpaqueExtrinsic, Perbill, Percent,
    generic::BlockId,
    traits::{Block as BlockT, Header as HeaderT, NumberFor},
};
use sp_state_machine::Backend;
use sp_timestamp::Timestamp;
use std::{
    ops::Deref,
    sync::{Arc, LazyLock},
    time::{self, SystemTime, UNIX_EPOCH},
};
use testing::{
    client::{
        Backend as TestBackend, Client as TestClient, ClientBlockImportExt, RuntimeExecutor,
        TestClientBuilder, TestClientBuilderExt,
    },
    keyring::{CheckedExtrinsic, alice, bob, sign, signed_extra},
};
use vara_runtime::{
    AccountId, Runtime, RuntimeApi as RA, RuntimeCall, SLOT_DURATION, UncheckedExtrinsic, VERSION,
};

type TestProposal = sp_consensus::Proposal<TestBlock, ()>;

static EXECUTOR: LazyLock<RuntimeExecutor> = LazyLock::new(|| RuntimeExecutor::builder().build());

const SOURCE: TransactionSource = TransactionSource::External;
const DEFAULT_GAS_LIMIT: u64 = 10_000_000_000;

fn chain_event<B: BlockT>(header: B::Header) -> ChainEvent<B>
where
    NumberFor<B>: From<u32>,
{
    ChainEvent::NewBestBlock {
        hash: header.hash(),
        tree_route: None,
    }
}

fn pre_digest(slot: u64, authority_index: u32) -> Digest {
    Digest {
        logs: vec![DigestItem::PreRuntime(
            BABE_ENGINE_ID,
            PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                slot: Slot::from(slot),
                authority_index,
            })
            .encode(),
        )],
    }
}

fn checked_extrinsics<F>(
    n: u32,
    signer: AccountId,
    starting_nonce: u32,
    f: F,
) -> Vec<CheckedExtrinsic>
where
    F: Fn() -> RuntimeCall,
{
    let last_nonce = starting_nonce + n;
    (starting_nonce..last_nonce)
        .map(|nonce| CheckedExtrinsic {
            signed: Some((signer.clone(), signed_extra(nonce))),
            function: f(),
        })
        .collect()
}

fn sign_extrinsics<E>(
    extrinsics: Vec<CheckedExtrinsic>,
    spec_version: u32,
    tx_version: u32,
    best_hash: [u8; 32],
) -> Vec<E>
where
    E: From<UncheckedExtrinsic>,
{
    extrinsics
        .into_iter()
        .map(|x| sign(x, spec_version, tx_version, best_hash, None).into())
        .collect()
}

fn salt() -> [u8; 16] {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_le_bytes()
}

enum TestCall {
    Noop,
    InitLoop(u64),
    ToggleRunQueue(bool),
    ExhaustResources,
}

struct CallBuilder {
    call: TestCall,
}
impl CallBuilder {
    pub fn noop() -> Self {
        Self {
            call: TestCall::Noop,
        }
    }
    pub fn long_init(count: u64) -> Self {
        Self {
            call: TestCall::InitLoop(count),
        }
    }
    pub fn toggle_run_queue(value: bool) -> Self {
        Self {
            call: TestCall::ToggleRunQueue(value),
        }
    }
    pub fn exhaust_resources() -> Self {
        Self {
            call: TestCall::ExhaustResources,
        }
    }
    fn build(self) -> RuntimeCall {
        match self.call {
            TestCall::Noop => RuntimeCall::Gear(pallet_gear::Call::upload_program {
                code: WASM_BINARY.to_vec(),
                salt: salt().to_vec(),
                init_payload: Scheme::direct(Calls::builder().noop()).encode(),
                gas_limit: DEFAULT_GAS_LIMIT,
                value: 0,
                keep_alive: false,
            }),
            TestCall::InitLoop(count) => RuntimeCall::Gear(pallet_gear::Call::upload_program {
                code: WASM_BINARY.to_vec(),
                salt: salt().to_vec(),
                init_payload: Scheme::direct(Calls::builder().write_in_loop(count)).encode(),
                gas_limit: DEFAULT_GAS_LIMIT,
                value: 0,
                keep_alive: false,
            }),
            TestCall::ToggleRunQueue(value) => RuntimeCall::Sudo(pallet_sudo::Call::sudo {
                call: Box::new(RuntimeCall::Gear(pallet_gear::Call::set_execute_inherent {
                    value,
                })),
            }),
            TestCall::ExhaustResources => {
                // Using 75% of the max possible weight so that two such calls will inevitably
                // exhaust block resources while one call will very likely fit in.
                RuntimeCall::GearDebug(pallet_gear_debug::Call::exhaust_block_resources {
                    fraction: Percent::from_percent(75),
                })
            }
        }
    }
}

pub(crate) fn init_logger() {
    let _ = tracing_subscriber::fmt::try_init();
}

pub fn init() -> (
    Arc<TestClient>,
    Arc<TestBackend>,
    Arc<FullPool<TestBlock, TestClient>>,
    sp_core::testing::TaskExecutor,
    [u8; 32],
) {
    let client_builder = TestClientBuilder::new();
    let backend = client_builder.backend();
    let client = Arc::new(client_builder.build(Some(EXECUTOR.clone())));
    let spawner = sp_core::testing::TaskExecutor::new();
    let txpool = BasicPool::new_full(
        Default::default(),
        true.into(),
        None,
        spawner.clone(),
        client.clone(),
    );

    let genesis_hash =
        <[u8; 32]>::try_from(&client.info().best_hash[..]).expect("H256 is a 32 byte type");
    (client, backend, txpool, spawner, genesis_hash)
}

pub fn create_proposal<A>(
    client: Arc<TestClient>,
    _backend: Arc<TestBackend>,
    txpool: Arc<A>,
    spawner: sp_core::testing::TaskExecutor,
    parent_number: BlockNumber,
    deadline: time::Duration,
    now: Box<dyn Fn() -> time::Instant + Send + Sync>,
    max_gas: Option<u64>,
) -> TestProposal
where
    A: TransactionPool<Block = TestBlock> + 'static,
{
    let mut proposer_factory = ProposerFactory::new(
        spawner.clone(),
        client.clone(),
        txpool.clone(),
        None,
        None,
        max_gas,
    );

    let hash = client
        .expect_block_hash_from_id(&BlockId::Number(parent_number))
        .unwrap();
    let proposer = proposer_factory.init_with_now(&client.expect_header(hash).unwrap(), now);

    let time_slot = parent_number as u64 + 1000_u64;
    let timestamp = Timestamp::new(time_slot * SLOT_DURATION + 100_u64);
    let timestamp_provider = sp_timestamp::InherentDataProvider::new(timestamp);
    let inherent_data =
        block_on(timestamp_provider.create_inherent_data()).expect("Create inherent data failed");

    let proposal =
        block_on(proposer.propose(inherent_data, pre_digest(time_slot, 0), deadline, None))
            .unwrap();

    // Import last block
    block_on(client.import(BlockOrigin::Own, proposal.block.clone())).unwrap();

    assert_eq!(client.info().best_hash, proposal.block.hash());

    proposal
}

fn submit_and_maintain<A>(client: Arc<TestClient>, txpool: Arc<A>, extrinsics: Vec<OpaqueExtrinsic>)
where
    A: MaintainedTransactionPool<Block = TestBlock> + 'static,
{
    let hash = client.info().best_hash;

    block_on(txpool.submit_at(hash, SOURCE, extrinsics)).unwrap();
    block_on(txpool.maintain(chain_event(
        client.expect_header(hash).expect("there should be header"),
    )));
}

#[test]
fn run_all_tests() {
    use basic_tests::*;

    const TESTS: &[fn()] = &[
        test_pseudo_inherent_placed_in_each_block,
        test_queue_remains_intact_if_processing_fails,
        test_block_max_gas_works,
        test_pseudo_inherent_discarded_from_txpool,
        test_block_builder_cloned_ok,
        test_proposal_timing_consistent,
        test_building_block_ceased_when_deadline_is_reached,
        test_no_panic_when_deadline_is_reached,
        test_proposed_storage_changes_match_execute_block_storage_changes,
        test_invalid_transactions_not_removed_when_skipping,
        test_building_block_ceased_when_block_limit_is_reached,
        test_transactions_keep_being_added_after_exhaust_resources_before_soft_deadline,
        test_skipping_only_up_to_some_limit_after_soft_deadline,
    ];

    init_logger();

    for &test in TESTS {
        test();
    }
}

fn test_pseudo_inherent_placed_in_each_block() {
    let (client, backend, txpool, spawner, genesis_hash) = init();

    let extrinsics = sign_extrinsics(
        checked_extrinsics(1, bob(), 0_u32, || CallBuilder::noop().build()),
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );

    submit_and_maintain(client.clone(), txpool.clone(), extrinsics);
    assert_eq!(txpool.ready().count(), 1);

    let current_block = client.info().best_number;

    let block = create_proposal(
        client.clone(),
        backend.clone(),
        txpool.clone(),
        spawner.clone(),
        current_block,
        time::Duration::from_millis(1500_u64),
        Box::new(time::Instant::now),
        None,
    )
    .block;

    // then
    // block should have exactly 3 txs: an inherent (timestamp), a normal and a mandatory one
    assert_eq!(block.extrinsics().len(), 3);
}

fn test_queue_remains_intact_if_processing_fails() {
    use sp_state_machine::IterArgs;

    let (client, backend, txpool, spawner, genesis_hash) = init();

    let mut checked = checked_extrinsics(5, bob(), 0_u32, || CallBuilder::noop().build());
    let nonce = 5_u32; // Bob's nonce for the future

    // Disable queue processing in Gear pallet as the root
    checked.push(CheckedExtrinsic {
        signed: Some((alice(), signed_extra(0))),
        function: CallBuilder::toggle_run_queue(false).build(),
    });
    let extrinsics = sign_extrinsics(
        checked,
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );
    submit_and_maintain(client.clone(), txpool.clone(), extrinsics);
    assert_eq!(txpool.ready().count(), 6);

    let current_block = client.info().best_number;

    let cell = Mutex::new((0_u32, time::Instant::now()));

    // Proposer's `self.now()` function increments the `Instant` by 0.02s each time it's called
    // which will allow us to place all our extrinsics in a single block
    let now = Box::new(move || {
        let mut value = cell.lock();
        let increment = 20_u64;
        let old = value.1;
        let new = old + time::Duration::from_millis(increment);
        *value = (value.0 + 1, new);
        old
    });

    let block = create_proposal(
        client.clone(),
        backend.clone(),
        txpool.clone(),
        spawner.clone(),
        current_block,
        time::Duration::from_millis(1500_u64),
        now,
        None,
    )
    .block;
    // Pseudo-inherent rolled back, therefore only have 1 inherent + 7 normal
    assert_eq!(block.extrinsics().len(), 7);

    let best_hash = block.hash();

    // Ensure message queue still has 5 messages
    let state = backend.state_at(best_hash).unwrap();
    let queue_entry_prefix = storage_prefix(
        pallet_gear_messenger::Pallet::<Runtime>::name().as_bytes(),
        "Dispatches".as_bytes(),
    );
    let mut queue_entry_args = IterArgs::default();
    queue_entry_args.prefix = Some(&queue_entry_prefix);

    let mut queue_len = 0_u32;

    state
        .keys(queue_entry_args)
        .unwrap()
        .for_each(|_k| queue_len += 1);
    assert_eq!(queue_len, 5);

    // Preparing block #2
    let extrinsics = sign_extrinsics(
        checked_extrinsics(3, bob(), nonce, || CallBuilder::noop().build()),
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );
    submit_and_maintain(client.clone(), txpool.clone(), extrinsics);
    assert_eq!(txpool.ready().count(), 3);

    let current_block = client.info().best_number;

    let block = create_proposal(
        client.clone(),
        backend.clone(),
        txpool.clone(),
        spawner.clone(),
        current_block,
        time::Duration::from_millis(1500_u64),
        Box::new(time::Instant::now),
        None,
    )
    .block;
    // Terminal extrinsic rolled back, therefore only have 1 inherent + another 3 normal
    assert_eq!(block.extrinsics().len(), 4);

    let best_hash = block.hash();

    let state = backend.state_at(best_hash).unwrap();
    // Ensure message queue has not been drained again, and now has 8 messages
    let mut queue_len = 0_u32;
    let mut queue_entry_args = IterArgs::default();
    queue_entry_args.prefix = Some(&queue_entry_prefix);
    state
        .keys(queue_entry_args)
        .unwrap()
        .for_each(|_k| queue_len += 1);
    assert_eq!(queue_len, 8);
}

fn test_block_max_gas_works() {
    use pallet_gear_builtin::WeightInfo;
    use sp_state_machine::IterArgs;

    // Amount of gas burned in each block (even empty) by default
    const FIXED_BLOCK_GAS: u64 = 25_000_000;

    let (client, backend, txpool, spawner, genesis_hash) = init();

    // Prepare block #1
    // Proposing it's empty to workaround block author in RPC.
    submit_and_maintain(client.clone(), txpool.clone(), vec![]);

    let current_block = client.info().best_number;

    let block = create_proposal(
        client.clone(),
        backend.clone(),
        txpool.clone(),
        spawner.clone(),
        current_block,
        time::Duration::from_millis(1500_u64),
        Box::new(time::Instant::now),
        None,
    )
    .block;

    let best_hash = block.hash();

    let api = client.runtime_api();
    let gear_core::rpc::GasInfo { min_limit, .. } = api
        .calculate_gas_info(
            best_hash,
            sp_core::H256::from(alice().as_ref()),
            pallet_gear::HandleKind::Init(WASM_BINARY.to_vec()),
            Scheme::direct(Calls::builder().noop()).encode(),
            0,
            true,
            None,
            None,
        )
        .unwrap()
        .unwrap();

    // Gas cost of a built router instance creation
    let builtin_dispatcher_cost =
        <Runtime as pallet_gear_builtin::Config>::WeightInfo::create_dispatcher().ref_time();
    // Just enough to fit 2 messages
    let max_gas = Some(2 * min_limit + FIXED_BLOCK_GAS + builtin_dispatcher_cost + 100);

    // Preparing block #2
    // Creating 5 extrinsics
    let checked = checked_extrinsics(5, bob(), 0, || CallBuilder::noop().build());
    let extrinsics = sign_extrinsics(
        checked,
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );
    submit_and_maintain(client.clone(), txpool.clone(), extrinsics);

    let current_block = client.info().best_number;

    let block = create_proposal(
        client.clone(),
        backend.clone(),
        txpool.clone(),
        spawner.clone(),
        current_block,
        time::Duration::from_millis(1500_u64),
        Box::new(time::Instant::now),
        max_gas,
    )
    .block;

    // All extrinsics have been included in the block: 1 inherent + 5 normal + 1 terminal
    assert_eq!(block.extrinsics().len(), 7);

    let state = backend.state_at(block.hash()).unwrap();
    // Ensure message queue still has 5 messages as none of the messages fit into the gas allowance
    let queue_entry_prefix = storage_prefix(
        pallet_gear_messenger::Pallet::<Runtime>::name().as_bytes(),
        "Dispatches".as_bytes(),
    );
    let mut queue_entry_args = IterArgs::default();
    queue_entry_args.prefix = Some(&queue_entry_prefix);

    let queue_len = state.keys(queue_entry_args).unwrap().count();

    // 2 out of 5 messages have been processed, 3 remain in the queue
    assert_eq!(queue_len, 3);

    let programs_prefix = storage_prefix(
        pallet_gear_program::Pallet::<Runtime>::name().as_bytes(),
        "ProgramStorage".as_bytes(),
    );
    let mut iter_args = IterArgs::default();
    iter_args.prefix = Some(&programs_prefix);

    // The fact that 2 init messages out of 5 have been processed means
    // that there should be 2 inited programs.
    let inited_count = state.pairs(iter_args).unwrap().fold(0u32, |count, pair| {
        let value = match pair {
            Ok((_key, value)) => value,
            _ => return count,
        };

        match Program::<BlockNumber>::decode(&mut &value[..]) {
            Ok(p) if p.is_initialized() => count + 1,
            _ => count,
        }
    });
    assert_eq!(inited_count, 2);
}

fn test_pseudo_inherent_discarded_from_txpool() {
    let (client, backend, txpool, spawner, genesis_hash) = init();

    // Create Gear::run() extrinsic - both unsigned and signed
    let unsigned_gear_run_xt =
        UncheckedExtrinsic::new_unsigned(RuntimeCall::Gear(pallet_gear::Call::run {
            max_gas: None,
        }));
    let signed_gear_run_xt = sign(
        CheckedExtrinsic {
            signed: Some((bob(), signed_extra(0))),
            function: RuntimeCall::Gear(pallet_gear::Call::run { max_gas: None }),
        },
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
        None,
    );
    // A `DispatchClass::Normal` extrinsic - supposed to end up in the txpool
    let legit_xt = sign(
        CheckedExtrinsic {
            signed: Some((alice(), signed_extra(0))),
            function: CallBuilder::noop().build(),
        },
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
        None,
    );

    let extrinsics = vec![
        unsigned_gear_run_xt.into(),
        signed_gear_run_xt.into(),
        legit_xt.into(),
    ];
    submit_and_maintain(client.clone(), txpool.clone(), extrinsics);
    assert_eq!(txpool.ready().count(), 1);

    let current_block = client.info().best_number;

    let block = create_proposal(
        client.clone(),
        backend.clone(),
        txpool.clone(),
        spawner.clone(),
        current_block,
        time::Duration::from_millis(1500_u64),
        Box::new(time::Instant::now),
        None,
    )
    .block;

    // Both mandatory extrinsics should have been discarded, therefore there are only 3 txs
    // in the block: 1 timestamp inherent + 1 normal extrinsic + 1 terminal
    assert_eq!(block.extrinsics().len(), 3);
}

fn test_block_builder_cloned_ok() {
    let (client, _, _, _, genesis_hash) = init();

    let extrinsics = sign_extrinsics(
        checked_extrinsics(5, bob(), 0, || CallBuilder::noop().build()),
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );

    let mut block_builder = BlockBuilderBuilder::new(client.as_ref())
        .on_parent_block(genesis_hash.into())
        .with_parent_block_number(0)
        .with_inherent_digests(pre_digest(1, 0))
        .build()
        .unwrap();

    extrinsics.into_iter().for_each(|xt: OpaqueExtrinsic| {
        log::info!("{:?}", &xt);
        assert_ok!(block_builder.push(xt));
    });

    assert_eq!(block_builder.extrinsics().len(), 5);

    // At this point the overlay wrapped in the `Api` instance has some changes
    let fresh_block_builder = BlockBuilderBuilder::new(client.as_ref())
        .on_parent_block(genesis_hash.into())
        .with_parent_block_number(0)
        .with_inherent_digests(pre_digest(1, 0))
        .build()
        .unwrap();

    let cloned_block_builder = block_builder.clone();

    let (ext_1, api_1, ver_1, phash_1, bd_1, hsize_1) = block_builder.deconstruct();
    let (ext_2, api_2, ver_2, phash_2, bd_2, hsize_2) = cloned_block_builder.deconstruct();

    // Assert that the components are equal but different
    assert_eq!(ext_1, ext_2);
    assert_ne!(ext_1.as_ptr(), ext_2.as_ptr());
    let api_1_ptr: *const <RA as sp_api::ConstructRuntimeApi<_, Client<_, _, _, RA>>>::RuntimeApi =
        api_1.deref();
    let api_2_ptr: *const <RA as sp_api::ConstructRuntimeApi<_, Client<_, _, _, RA>>>::RuntimeApi =
        api_2.deref();
    assert_ne!(api_1_ptr, api_2_ptr);

    // Reconstruct original block builders
    let block_builder = BlockBuilder::<'_, _, Client<_, _, _, RA>>::from_parts(
        ext_1, api_1, ver_1, phash_1, bd_1, hsize_1,
    );
    let cloned_block_builder = BlockBuilder::<'_, _, Client<_, _, _, RA>>::from_parts(
        ext_2, api_2, ver_2, phash_2, bd_2, hsize_2,
    );

    let changes_1 = block_builder.into_storage_changes().unwrap();
    let changes_2 = cloned_block_builder.into_storage_changes().unwrap();
    let changes_3 = fresh_block_builder.into_storage_changes().unwrap();

    // Assert that the original and the cloned block builders produce same storage changes
    assert_eq!(
        changes_1.transaction_storage_root,
        changes_2.transaction_storage_root
    );
    // that are different from what builder created from scratch produces
    assert_ne!(
        changes_1.transaction_storage_root,
        changes_3.transaction_storage_root
    );
}

fn test_proposal_timing_consistent() {
    use sp_state_machine::IterArgs;

    let (client, backend, txpool, spawner, genesis_hash) = init();

    // Disable queue processing in block #1
    let mut checked = vec![CheckedExtrinsic {
        signed: Some((alice(), signed_extra(0))),
        function: CallBuilder::toggle_run_queue(false).build(),
    }];

    // Creating a bunch of extrinsics that will put N time-consuming init messages
    // to the message queue. The number of extrinsics should better allow all of
    // them to fit in one block to know deterministically the number of messages.
    // Empirically, 50 extrinsics is a good number.
    checked.extend(checked_extrinsics(50, bob(), 0, || {
        // TODO: this is a "hand-wavy" workaround to have a long-running init message.
        // Should be replaced with a more reliable solution (like zero-cost syscalls
        // in init message that would guarantee incorrect gas estimation)
        CallBuilder::long_init(500_u64).build()
    }));
    let extrinsics = sign_extrinsics(
        checked,
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );

    submit_and_maintain(client.clone(), txpool.clone(), extrinsics);

    let current_block = client.info().best_number;
    let max_duration = 15_000_u64; // sufficient time

    let block = create_proposal(
        client.clone(),
        backend.clone(),
        txpool.clone(),
        spawner.clone(),
        current_block,
        time::Duration::from_millis(max_duration),
        Box::new(time::Instant::now),
        None,
    )
    .block;

    let state = backend.state_at(block.hash()).unwrap();

    let queue_entry_prefix = storage_prefix(
        pallet_gear_messenger::Pallet::<Runtime>::name().as_bytes(),
        "Dispatches".as_bytes(),
    );
    let mut queue_entry_args = IterArgs::default();
    queue_entry_args.prefix = Some(&queue_entry_prefix);

    let queue_len_at_1 = state.keys(queue_entry_args).unwrap().count();

    // Preparing block #2
    // Re-enable queue processing in block #2
    let extrinsics = sign_extrinsics(
        vec![CheckedExtrinsic {
            signed: Some((alice(), signed_extra(1))),
            function: CallBuilder::toggle_run_queue(true).build(),
        }],
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );
    submit_and_maintain(client.clone(), txpool.clone(), extrinsics);

    let current_block = client.info().best_number;

    // Simulate the situation when the `Gear::run()` takes longer time to execute than
    // the actual time that remains till the deadline.
    // Here we set `max_duration` to 0.3s to try to hit the timeout during the queue processing.
    let max_duration = 300_u64;
    let cell = Arc::new(Mutex::new((0, time::Instant::now())));
    // The time function that makes longer jumps in time every time it's called
    // (starting from the third call)
    let now = Box::new(move || {
        let mut value = cell.lock();
        let (called, old) = *value;
        let increase = if called > 1 {
            time::Duration::from_millis(max_duration)
                .mul_f32(0.2)
                .mul_f32(called as f32 - 1.0)
        } else {
            time::Duration::from_millis(0)
        };
        *value = (called + 1, old + increase);
        old
    });

    let block = create_proposal(
        client.clone(),
        backend.clone(),
        txpool.clone(),
        spawner.clone(),
        current_block,
        time::Duration::from_millis(max_duration),
        now,
        None,
    )
    .block;

    let state = backend.state_at(block.hash()).unwrap();

    // Check that the message queue has all messages pushed to it
    let queue_entry_prefix = storage_prefix(
        pallet_gear_messenger::Pallet::<Runtime>::name().as_bytes(),
        "Dispatches".as_bytes(),
    );
    let mut queue_entry_args = IterArgs::default();
    queue_entry_args.prefix = Some(&queue_entry_prefix);

    let queue_len = state.keys(queue_entry_args).unwrap().count();

    // `Gear::run()` should have triggered timeout, therefore the
    // queue should still have all the original messages
    assert_eq!(queue_len, queue_len_at_1);

    // Let the `Gear::run()` thread a little more time to finish
    std::thread::sleep(time::Duration::from_millis(500));

    // Preparing block #3
    submit_and_maintain(client.clone(), txpool.clone(), vec![]);

    // In the meantime make sure we can still keep creating blocks
    // This time we set the deadline to a very high value to ensure that all messages go through.
    let max_duration = 15_000_u64;
    let current_block = client.info().best_number;

    let block = create_proposal(
        client.clone(),
        backend.clone(),
        txpool.clone(),
        spawner.clone(),
        current_block,
        time::Duration::from_millis(max_duration),
        Box::new(time::Instant::now),
        None,
    )
    .block;

    let state = backend.state_at(block.hash()).unwrap();

    let mut queue_entry_args = IterArgs::default();
    queue_entry_args.prefix = Some(&queue_entry_prefix);

    let queue_len = state.keys(queue_entry_args).unwrap().count();
    assert_eq!(queue_len, 0);
}

// Original tests from Substrate's `sc-basic-authorship` crate adjusted for actual Vara runtime
mod basic_tests {
    use super::*;

    fn extrinsic<E>(nonce: u32, signer: &AccountId, genesis_hash: [u8; 32]) -> E
    where
        E: From<UncheckedExtrinsic> + Clone,
    {
        sign_extrinsics::<E>(
            checked_extrinsics(1, signer.clone(), nonce, || CallBuilder::noop().build()),
            VERSION.spec_version,
            VERSION.transaction_version,
            genesis_hash,
        )[0]
        .clone()
    }

    fn exhausts_resources_extrinsic<E>(nonce: u32, signer: &AccountId, genesis_hash: [u8; 32]) -> E
    where
        E: From<UncheckedExtrinsic> + Clone,
    {
        sign_extrinsics::<E>(
            checked_extrinsics(1, signer.clone(), nonce, || {
                CallBuilder::exhaust_resources().build()
            }),
            VERSION.spec_version,
            VERSION.transaction_version,
            genesis_hash,
        )[0]
        .clone()
    }

    fn disable_gear_run<E>(nonce: u32, genesis_hash: [u8; 32]) -> E
    where
        E: From<UncheckedExtrinsic> + Clone,
    {
        sign_extrinsics::<E>(
            vec![CheckedExtrinsic {
                signed: Some((alice(), signed_extra(nonce))),
                function: CallBuilder::toggle_run_queue(false).build(),
            }],
            VERSION.spec_version,
            VERSION.transaction_version,
            genesis_hash,
        )[0]
        .clone()
    }

    pub(super) fn test_building_block_ceased_when_deadline_is_reached() {
        let (client, backend, txpool, spawner, genesis_hash) = init();

        let mut extrinsics = vec![disable_gear_run(0, genesis_hash)];

        extrinsics.extend(sign_extrinsics(
            checked_extrinsics(2, alice(), 1_u32, || CallBuilder::noop().build()),
            VERSION.spec_version,
            VERSION.transaction_version,
            genesis_hash,
        ));
        submit_and_maintain(client.clone(), txpool.clone(), extrinsics);

        let cell = Mutex::new((0_u32, time::Instant::now()));

        // Proposer's `self.now()` function increments the `Instant` by 1s each time it's called
        // (starting from the moment we enter tx processing loop, that is from the 4th call)
        let now = Box::new(move || {
            let mut value = cell.lock();
            let increment = if value.0 < 3 { 0_u64 } else { 1_u64 };
            let old = value.1;
            let new = old + time::Duration::from_secs(increment);
            *value = (value.0 + 1, new);
            old
        });

        // `max_duration` of 3s will be converted into 0.7s hard deadline inside extrinsics loop:
        //  (2/3) * 3s * 0.35 = 0.7s, which will allow to include in the block 1 normal extrinsic
        let max_duration = 3000_u64;

        let current_block = client.info().best_number;

        let block = create_proposal(
            client.clone(),
            backend.clone(),
            txpool.clone(),
            spawner.clone(),
            current_block,
            time::Duration::from_millis(max_duration),
            now,
            None,
        )
        .block;

        // then
        // The block has 2 txs: the timestamp inherent and one normal.
        // The pseudo-inherent is disabled.
        assert_eq!(block.extrinsics().len(), 2);

        assert_eq!(txpool.ready().count(), 3);
    }

    pub(super) fn test_no_panic_when_deadline_is_reached() {
        let (client, backend, txpool, spawner, _) = init();

        let cell = Mutex::new((false, time::Instant::now()));
        // The `proposer.now()` that increments the `Instant` by 160s each time it's called
        let now = Box::new(move || {
            let mut value = cell.lock();
            if !value.0 {
                value.0 = true;
                return value.1;
            }
            let new = value.1 + time::Duration::from_secs(160);
            *value = (true, new);
            new
        });
        let max_duration = 1000_u64; // 1s

        let _ = create_proposal(
            client.clone(),
            backend.clone(),
            txpool.clone(),
            spawner.clone(),
            0_u32,
            time::Duration::from_millis(max_duration),
            now,
            None,
        )
        .block;
    }

    pub(super) fn test_proposed_storage_changes_match_execute_block_storage_changes() {
        let (client, backend, txpool, spawner, genesis_hash) = init();

        let extrinsics = sign_extrinsics(
            checked_extrinsics(1, bob(), 0_u32, || CallBuilder::noop().build()),
            VERSION.spec_version,
            VERSION.transaction_version,
            genesis_hash,
        );

        submit_and_maintain(client.clone(), txpool.clone(), extrinsics);

        let proposal = create_proposal(
            client.clone(),
            backend.clone(),
            txpool.clone(),
            spawner.clone(),
            0_u32,
            time::Duration::from_millis(1500),
            Box::new(time::Instant::now),
            None,
        );

        // then
        // 1 inherent + 1 signed extrinsic + 1 terminal unsigned one
        assert_eq!(proposal.block.extrinsics().len(), 3);

        let api = client.runtime_api();
        let genesis_hash = genesis_hash.into();
        api.execute_block(genesis_hash, proposal.block).unwrap();

        let state = backend.state_at(genesis_hash).unwrap();

        let storage_changes = api.into_storage_changes(&state, genesis_hash).unwrap();

        assert_eq!(
            proposal.storage_changes.transaction_storage_root,
            storage_changes.transaction_storage_root,
        );

        let queue_head_key = storage_prefix(
            pallet_gear_messenger::Pallet::<Runtime>::name().as_bytes(),
            "Head".as_bytes(),
        );
        // Ensure message queue is empty given the terminal extrinsic completed successfully
        assert!(state.storage(&queue_head_key[..]).unwrap().is_none());
    }

    pub(super) fn test_invalid_transactions_not_removed_when_skipping() {
        let (client, backend, txpool, spawner, genesis_hash) = init();

        let alice = alice();

        let extrinsics = vec![
            extrinsic(0, &alice, genesis_hash),
            extrinsic(1, &alice, genesis_hash),
            exhausts_resources_extrinsic(2, &alice, genesis_hash),
            extrinsic(3, &alice, genesis_hash),
            exhausts_resources_extrinsic(4, &alice, genesis_hash),
            extrinsic(5, &alice, genesis_hash),
            extrinsic(6, &alice, genesis_hash),
        ];

        submit_and_maintain(client.clone(), txpool.clone(), extrinsics);
        assert_eq!(txpool.ready().count(), 7);

        let block = create_proposal(
            client.clone(),
            backend.clone(),
            txpool.clone(),
            spawner.clone(),
            0_u32,
            time::Duration::from_millis(1500_u64),
            Box::new(time::Instant::now),
            None,
        )
        .block;

        // then
        // block should have some extrinsics although we have some more in the pool.
        assert_eq!(txpool.ready().count(), 7);
        assert_eq!(block.extrinsics().len(), 6);

        // Preparing block #2
        submit_and_maintain(client.clone(), txpool.clone(), vec![]);
        assert_eq!(txpool.ready().count(), 3);

        let current_block = client.info().best_number;

        let block = create_proposal(
            client.clone(),
            backend.clone(),
            txpool.clone(),
            spawner.clone(),
            current_block,
            time::Duration::from_millis(1500_u64),
            Box::new(time::Instant::now),
            None,
        )
        .block;

        // 1 normal extrinsic should still make it into block (together with inherents):
        assert_eq!(txpool.ready().count(), 3);
        assert_eq!(block.extrinsics().len(), 5);
    }

    pub(super) fn test_building_block_ceased_when_block_limit_is_reached() {
        let (client, _, txpool, spawner, genesis_hash) = init();

        let block_id = BlockId::number(0);
        let genesis_header = client
            .header(client.block_hash_from_id(&block_id).unwrap().unwrap())
            .expect("header get error")
            .expect("there should be header");

        let extrinsics_num = 5_usize;
        let extrinsics = sign_extrinsics(
            checked_extrinsics(extrinsics_num as u32, bob(), 0_u32, || {
                CallBuilder::noop().build()
            }),
            VERSION.spec_version,
            VERSION.transaction_version,
            genesis_hash,
        );

        let timestamp_extrinsic_size = 11_usize;
        let tolerance = Perbill::from_percent(20);
        let all_but_extrinsics = (genesis_header.encoded_size()
            + Vec::<OpaqueExtrinsic>::new().encoded_size()
            + timestamp_extrinsic_size) as u32;
        let block_limit = (all_but_extrinsics + tolerance * all_but_extrinsics) as usize
            + extrinsics
                .iter()
                .take(extrinsics_num - 1)
                .map(Encode::encoded_size)
                .sum::<usize>();

        let hashof0 = client.info().genesis_hash;

        block_on(txpool.submit_at(hashof0, SOURCE, extrinsics)).unwrap();

        block_on(txpool.maintain(chain_event(genesis_header.clone())));

        let mut proposer_factory = ProposerFactory::new(
            spawner.clone(),
            client.clone(),
            txpool.clone(),
            None,
            None,
            None,
        );

        let proposer = block_on(proposer_factory.init(&genesis_header)).unwrap();

        // Give it enough time
        let deadline = time::Duration::from_secs(300_000);
        let timestamp = Timestamp::current();
        let timestamp_provider = sp_timestamp::InherentDataProvider::new(timestamp);
        let time_slot = timestamp.as_millis() / SLOT_DURATION;
        let inherent_data = block_on(timestamp_provider.create_inherent_data())
            .expect("Create inherent data failed");

        let block = block_on(proposer.propose(
            inherent_data.clone(),
            pre_digest(time_slot, 0),
            deadline,
            Some(block_limit),
        ))
        .map(|r| r.block)
        .unwrap();

        // Based on the block limit, one transaction shouldn't be included.
        // Instead, we have the timestamp and the pseudo-inherent.
        assert_eq!(block.extrinsics().len(), extrinsics_num - 1 + 2);

        let proposer = block_on(proposer_factory.init(&genesis_header)).unwrap();

        let block = block_on(proposer.propose(
            inherent_data.clone(),
            pre_digest(time_slot, 0),
            deadline,
            None,
        ))
        .map(|r| r.block)
        .unwrap();

        // Without a block limit we should include all of them + inherents
        assert_eq!(block.extrinsics().len(), extrinsics_num + 2);

        let mut proposer_factory = ProposerFactory::with_proof_recording(
            spawner.clone(),
            client.clone(),
            txpool.clone(),
            None,
            None,
            None,
        );

        let proposer = block_on(proposer_factory.init(&genesis_header)).unwrap();

        // Give it enough time
        let block = block_on(proposer.propose(
            inherent_data,
            pre_digest(time_slot, 0),
            deadline,
            Some(block_limit),
        ))
        .map(|r| r.block)
        .unwrap();

        // The block limit didn't changed, but we now include the proof in the estimation of the
        // block size and thus, we fit in the block one ordinary extrinsic less as opposed to
        // `extrinsics_num - 1` extrinsics we could fit earlier (mind the inherents, as usually).
        assert_eq!(block.extrinsics().len(), extrinsics_num - 2 + 2);
    }

    pub(super) fn test_transactions_keep_being_added_after_exhaust_resources_before_soft_deadline()
    {
        let (client, backend, txpool, spawner, genesis_hash) = init();

        let alice = alice();
        let bob = bob();

        let extrinsics = (0_usize..MAX_SKIPPED_TRANSACTIONS * 2)
            .map(|i| exhausts_resources_extrinsic(i as u32, &alice, genesis_hash))
            // and some transactions that are okay.
            .chain(
                (0_usize..MAX_SKIPPED_TRANSACTIONS)
                    .map(|i| extrinsic(i as u32, &bob, genesis_hash)),
            )
            .collect();

        submit_and_maintain(client.clone(), txpool.clone(), extrinsics);
        assert_eq!(txpool.ready().count(), MAX_SKIPPED_TRANSACTIONS * 3);

        // give it enough time so that deadline is never triggered.
        let max_duration = 900_000_u64;

        let current_block = client.info().best_number;

        let block = create_proposal(
            client.clone(),
            backend.clone(),
            txpool.clone(),
            spawner.clone(),
            current_block,
            time::Duration::from_millis(max_duration),
            Box::new(time::Instant::now),
            None,
        )
        .block;

        // then
        // MAX_SKIPPED_TRANSACTIONS + inherents have been included in the block
        assert_eq!(block.extrinsics().len(), MAX_SKIPPED_TRANSACTIONS + 3);
    }

    pub(super) fn test_skipping_only_up_to_some_limit_after_soft_deadline() {
        let (client, backend, txpool, spawner, genesis_hash) = init();

        let alice = alice();
        let extrinsics = (0_usize..MAX_SKIPPED_TRANSACTIONS + 2)
            .map(|i| exhausts_resources_extrinsic(i as u32, &alice, genesis_hash))
            .chain(
                (MAX_SKIPPED_TRANSACTIONS + 2..2_usize * MAX_SKIPPED_TRANSACTIONS + 2)
                    .map(|i| extrinsic(i as u32, &alice, genesis_hash)),
            )
            .collect();

        submit_and_maintain(client.clone(), txpool.clone(), extrinsics);
        assert_eq!(txpool.ready().count(), MAX_SKIPPED_TRANSACTIONS * 2 + 2);

        let cell = Arc::new(Mutex::new((0, time::Instant::now())));
        let cell2 = cell.clone();
        let max_duration = 1_000_000_u64; // more than enough time
        let now = Box::new(move || {
            let mut value = cell.lock();
            let (called, old) = *value;
            // add time after deadline is calculated internally (hence 1)
            let increase = if called == 1 {
                // We start after the `soft_deadline` should have already been reached.
                // `soft_deadline` is approx. 1/2 of `max_duration` * 0.35
                time::Duration::from_millis(max_duration) / 5
            } else {
                // but we make sure to never reach the actual deadline
                time::Duration::from_millis(0)
            };
            *value = (called + 1, old + increase);
            old
        });
        let current_block = client.info().best_number;

        let block = create_proposal(
            client.clone(),
            backend.clone(),
            txpool.clone(),
            spawner.clone(),
            current_block,
            time::Duration::from_millis(max_duration),
            now,
            None,
        )
        .block;

        // the block should have a single ordinary transaction despite more being in the pool
        assert_eq!(block.extrinsics().len(), 3);
        assert!(
            cell2.lock().0 > MAX_SKIPPED_TRANSACTIONS,
            "Not enough calls to current time, which indicates the test might have ended \
            because of deadline, not soft deadline"
        );
    }
}
