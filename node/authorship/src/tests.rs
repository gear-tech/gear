// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

#![allow(clippy::redundant_clone)]

use crate::{block_builder::BlockBuilder, ProposerFactory};

use codec::{Decode, Encode};
use common::Program;
use core::convert::TryFrom;
use demo_constructor::{Calls, Scheme, WASM_BINARY};
use frame_support::{assert_ok, storage::storage_prefix, traits::PalletInfoAccess};
use futures::executor::block_on;
use gear_runtime_common::constants::BANK_ADDRESS;
use pallet_gear_rpc_runtime_api::GearApi;
use runtime_primitives::BlockNumber;
use sc_client_api::{Backend as _, ExecutionStrategy};
use sc_service::client::Client;
use sc_transaction_pool::BasicPool;
use sc_transaction_pool_api::{
    ChainEvent, MaintainedTransactionPool, TransactionPool, TransactionSource,
};
use sp_api::{ApiExt, Core, ProvideRuntimeApi, StateBackend};
use sp_blockchain::HeaderBackend;
use sp_consensus::{BlockOrigin, Environment, Proposer};
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    Slot, BABE_ENGINE_ID,
};
use sp_inherents::InherentDataProvider;
use sp_runtime::{
    generic::BlockId,
    traits::{Block as BlockT, Header as HeaderT, NumberFor},
    Digest, DigestItem, OpaqueExtrinsic,
};
use sp_timestamp::Timestamp;
use std::{
    ops::Deref,
    sync::Arc,
    time::{self, SystemTime, UNIX_EPOCH},
};
use testing::{
    client::{ClientBlockImportExt, TestClientBuilder, TestClientBuilderExt},
    keyring::{alice, bob, sign, signed_extra, CheckedExtrinsic},
};
use vara_runtime::{
    AccountId, Runtime, RuntimeApi as RA, RuntimeCall, UncheckedExtrinsic, SLOT_DURATION, VERSION,
};

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
    genesis_hash: [u8; 32],
) -> Vec<E>
where
    E: From<UncheckedExtrinsic>,
{
    extrinsics
        .into_iter()
        .map(|x| sign(x, spec_version, tx_version, genesis_hash).into())
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
    DepositToBank,
    Noop,
    InitLoop(u64),
    ToggleRunQueue(bool),
}

struct CallBuilder {
    call: TestCall,
}
impl CallBuilder {
    pub fn deposit_to_bank() -> Self {
        Self {
            call: TestCall::DepositToBank,
        }
    }
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
    fn build(self) -> RuntimeCall {
        match self.call {
            TestCall::DepositToBank => RuntimeCall::Sudo(pallet_sudo::Call::sudo {
                call: Box::new(RuntimeCall::Balances(pallet_balances::Call::set_balance {
                    who: sp_runtime::MultiAddress::Id(AccountId::from(BANK_ADDRESS)),
                    new_free: 1_000_000_000_000_000,
                    new_reserved: 0,
                })),
            }),
            TestCall::Noop => RuntimeCall::Gear(pallet_gear::Call::upload_program {
                code: WASM_BINARY.to_vec(),
                salt: salt().to_vec(),
                init_payload: Scheme::direct(Calls::builder().noop()).encode(),
                gas_limit: DEFAULT_GAS_LIMIT,
                value: 0,
            }),
            TestCall::InitLoop(count) => RuntimeCall::Gear(pallet_gear::Call::upload_program {
                code: WASM_BINARY.to_vec(),
                salt: salt().to_vec(),
                init_payload: Scheme::direct(Calls::builder().write_in_loop(count)).encode(),
                gas_limit: DEFAULT_GAS_LIMIT,
                value: 0,
            }),
            TestCall::ToggleRunQueue(value) => RuntimeCall::Sudo(pallet_sudo::Call::sudo {
                call: Box::new(RuntimeCall::Gear(pallet_gear::Call::set_execute_inherent {
                    value,
                })),
            }),
        }
    }
}

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

macro_rules! init {
    {
        $client:ident,
        $backend:ident,
        $txpool:ident,
        $spawner:ident,
        $genesis_hash:ident
    } => {
        let client_builder =
            TestClientBuilder::new().set_execution_strategy(ExecutionStrategy::NativeWhenPossible);
        let $backend = client_builder.backend();
        let mut $client = Arc::new(client_builder.build());
        let $spawner = sp_core::testing::TaskExecutor::new();
        let $txpool = BasicPool::new_full(
            Default::default(),
            true.into(),
            None,
            $spawner.clone(),
            $client.clone(),
        );

        let $genesis_hash =
            <[u8; 32]>::try_from(&$client.info().best_hash[..]).expect("H256 is a 32 byte type");
    }
}

macro_rules! propose_block {
    {
        $client:ident,
        $backend:ident,
        $txpool:ident,
        $spawner:ident,
        $best_hash:ident,
        $block_id:ident,
        $extrinsics:ident,
        $timestamp:ident,
        $max_duration:ident,
        $max_gas:ident,
        $proposal:ident,
        {
            $( $txpool_ready:tt )*
        },
        {
            $( $final_checks:tt )*
        }
    } => {
        block_on($txpool.submit_at(&BlockId::number(0), SOURCE, $extrinsics)).unwrap();

        block_on(
            $txpool.maintain(chain_event(
                $client
                    .header(
                        $client
                            .block_hash_from_id(&$block_id)
                            .unwrap()
                            .unwrap(),
                    )
                    .expect("header get error")
                    .expect("there should be header"),
            )),
        );

        $( $txpool_ready )*

        let mut proposer_factory = ProposerFactory::new(
            $spawner.clone(),
            $client.clone(),
            $backend.clone(),
            $txpool.clone(),
            None,
            None,
            $max_gas,
        );

        let timestamp_provider = sp_timestamp::InherentDataProvider::new($timestamp);
        let time_slot = $timestamp.as_millis() / SLOT_DURATION;
        let inherent_data =
            block_on(timestamp_provider.create_inherent_data()).expect("Create inherent data failed");

        let proposer = block_on(
            proposer_factory.init(
                &$client
                    .header(
                        $client
                            .block_hash_from_id(&$block_id)
                            .unwrap()
                            .unwrap(),
                    )
                    .expect("Database error querying block #0")
                    .expect("Block #0 should exist"),
            ),
        )
        .expect("Proposer initialization failed");

        let $proposal = block_on(proposer.propose(
            inherent_data,
            pre_digest(time_slot, 0),
            time::Duration::from_millis($max_duration),
            None,
        ))
        .unwrap();

        // Importing last block
        block_on($client.import(BlockOrigin::Own, $proposal.block.clone())).unwrap();

        let $best_hash = $client.info().best_hash;
        assert_eq!($best_hash, $proposal.block.hash());

        $( $final_checks )*
    }
}

#[test]
fn custom_extrinsic_is_placed_in_each_block() {
    init_logger();

    init!(client, backend, txpool, spawner, genesis_hash);

    // Test prelude: extrinsics, timeouts etc.
    let extrinsics = sign_extrinsics(
        checked_extrinsics(1, bob(), 0_u32, || CallBuilder::noop().build()),
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );
    let num_extrinsics = extrinsics.len();

    let block_id = BlockId::number(0);
    let timestamp = Timestamp::current();
    let max_gas = None;
    let max_duration = 1500_u64;

    propose_block!(
        client,
        backend,
        txpool,
        spawner,
        best_hash,
        block_id,
        extrinsics,
        timestamp,
        max_duration,
        max_gas,
        proposal,
        {
            let num_ready = txpool.ready().count();
            assert_eq!(num_ready, num_extrinsics);
        },
        {
            // then
            // block should have exactly 3 txs: an inherent (timestamp), a normal and a mandatory one
            assert_eq!(proposal.block.extrinsics().len(), 3);
        }
    );
}

#[test]
fn proposed_storage_changes_match_execute_block_storage_changes() {
    init_logger();

    init!(client, backend, txpool, spawner, genesis_hash);

    let extrinsics = sign_extrinsics(
        checked_extrinsics(1, bob(), 0_u32, || CallBuilder::noop().build()),
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );

    let block_id = BlockId::number(0);
    let timestamp = Timestamp::current();
    let max_gas = None;
    let max_duration = 1500_u64;

    propose_block!(
        client,
        backend,
        txpool,
        spawner,
        best_hash,
        block_id,
        extrinsics,
        timestamp,
        max_duration,
        max_gas,
        proposal,
        {},
        {
            // then
            // 1 inherent + 1 signed extrinsic + 1 terminal unsigned one
            assert_eq!(proposal.block.extrinsics().len(), 3);

            let api = client.runtime_api();
            api.execute_block(genesis_hash.into(), proposal.block)
                .unwrap();
            let state = backend.state_at(best_hash).unwrap();

            let storage_changes = api.into_storage_changes(&state, best_hash).unwrap();

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
    );
}

#[test]
fn queue_remains_intact_if_processing_fails() {
    use sp_state_machine::IterArgs;

    init_logger();

    init!(client, backend, txpool, spawner, genesis_hash);

    // Create an extrinsic that prefunds the bank account
    let pre_fund_bank_xt = CheckedExtrinsic {
        signed: Some((alice(), signed_extra(0))),
        function: CallBuilder::deposit_to_bank().build(),
    };

    let mut checked = vec![pre_fund_bank_xt];
    checked.extend(checked_extrinsics(5, bob(), 0_u32, || CallBuilder::noop().build()).into_iter());
    let nonce = 5_u32; // Bob's nonce for the future

    // Disable queue processing in Gear pallet as the root
    checked.push(CheckedExtrinsic {
        signed: Some((alice(), signed_extra(1))),
        function: CallBuilder::toggle_run_queue(false).build(),
    });
    let extrinsics = sign_extrinsics(
        checked,
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );

    let block_id = BlockId::number(0);
    let timestamp = Timestamp::current();
    let max_gas = None;
    let max_duration = 1500_u64;

    propose_block!(
        client,
        backend,
        txpool,
        spawner,
        best_hash,
        block_id,
        extrinsics,
        timestamp,
        max_duration,
        max_gas,
        proposal,
        {},
        {
            // Terminal extrinsic rolled back, therefore only have 1 inherent + 6 normal
            assert_eq!(proposal.block.extrinsics().len(), 8);

            // Importing block #1
            block_on(client.import(BlockOrigin::Own, proposal.block.clone())).unwrap();

            let best_hash = client.info().best_hash;
            assert_eq!(best_hash, proposal.block.hash());

            let state = backend.state_at(best_hash).unwrap();
            // Ensure message queue still has 5 messages
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
        }
    );

    // Preparing block #2
    let block_id = BlockId::Hash(best_hash);
    let extrinsics = sign_extrinsics(
        checked_extrinsics(3, bob(), nonce, || CallBuilder::noop().build()),
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );
    let timestamp = Timestamp::new(timestamp.as_millis() + SLOT_DURATION);

    propose_block!(
        client,
        backend,
        txpool,
        spawner,
        best_hash,
        block_id,
        extrinsics,
        timestamp,
        max_duration,
        max_gas,
        proposal,
        {},
        {
            // Terminal extrinsic rolled back, therefore only have 1 inherent + 3 normal
            assert_eq!(proposal.block.extrinsics().len(), 4);

            // Importing block #2
            block_on(client.import(BlockOrigin::Own, proposal.block.clone())).unwrap();

            let state = backend.state_at(best_hash).unwrap();
            // Ensure message queue has not been drained and has now 8 messages
            let mut queue_len = 0_u32;
            let mut queue_entry_args = IterArgs::default();
            queue_entry_args.prefix = Some(&queue_entry_prefix);
            state
                .keys(queue_entry_args)
                .unwrap()
                .for_each(|_k| queue_len += 1);
            assert_eq!(queue_len, 8);
        }
    );
}

#[test]
fn block_max_gas_works() {
    use sp_state_machine::IterArgs;

    // Amount of gas burned in each block (even empty) by default
    const FIXED_BLOCK_GAS: u64 = 25_000_000;

    init_logger();

    init!(client, backend, txpool, spawner, genesis_hash);

    // Prepare block #1
    let block_id = BlockId::number(0);
    let timestamp = Timestamp::current();
    let mut max_gas = None;
    let max_duration = 1500_u64;
    // Create an extrinsic that prefunds the bank account
    let extrinsics = vec![sign(
        CheckedExtrinsic {
            signed: Some((alice(), signed_extra(0))),
            function: CallBuilder::deposit_to_bank().build(),
        },
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    )
    .into()];

    propose_block!(
        client,
        backend,
        txpool,
        spawner,
        best_hash,
        block_id,
        extrinsics,
        timestamp,
        max_duration,
        max_gas,
        proposal,
        {},
        {
            let api = client.runtime_api();
            let gear_core::gas::GasInfo { min_limit, .. } = api
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
            // Just enough to fit 2 messages
            max_gas = Some(2 * min_limit + FIXED_BLOCK_GAS + 100);
        }
    );

    // Preparing block #2
    // Creating 5 extrinsics
    // let checked = checked_extrinsics(5, bob(), 0, || CallBuilder::noop().build());
    let checked = checked_extrinsics(5, bob(), 0, || CallBuilder::noop().build());
    let extrinsics = sign_extrinsics(
        checked,
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );

    let block_id = BlockId::Hash(best_hash);
    let timestamp = Timestamp::new(timestamp.as_millis() + SLOT_DURATION);

    propose_block!(
        client,
        backend,
        txpool,
        spawner,
        best_hash,
        block_id,
        extrinsics,
        timestamp,
        max_duration,
        max_gas,
        proposal,
        {},
        {
            // All extrinsics have been included in the block: 1 inherent + 5 normal + 1 terminal
            assert_eq!(proposal.block.extrinsics().len(), 7);

            // Importing block #2
            block_on(client.import(BlockOrigin::Own, proposal.block.clone())).unwrap();

            let state = backend.state_at(best_hash).unwrap();
            // Ensure message queue still has 5 messages as none of the messages fit into the gas allownce
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
    );
}

#[test]
fn terminal_extrinsic_discarded_from_txpool() {
    init_logger();

    init!(client, backend, txpool, spawner, genesis_hash);

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
    );
    // A `DispatchClass::Normal` exrinsic - supposed to end up in the txpool
    let legit_xt = sign(
        CheckedExtrinsic {
            signed: Some((alice(), signed_extra(0))),
            function: CallBuilder::deposit_to_bank().build(),
        },
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );

    let extrinsics = vec![
        unsigned_gear_run_xt.into(),
        signed_gear_run_xt.into(),
        legit_xt.into(),
    ];

    let block_id = BlockId::number(0);
    let timestamp = Timestamp::current();
    let max_gas = None;
    let max_duration = 1500_u64;

    propose_block!(
        client,
        backend,
        txpool,
        spawner,
        best_hash,
        block_id,
        extrinsics,
        timestamp,
        max_duration,
        max_gas,
        proposal,
        {
            let num_ready = txpool.ready().count();
            assert_eq!(num_ready, 1);
        },
        {
            // Both mandatory extrinsics should have been discarded, therefore there are only 3 txs
            // in the block: 1 timestamp inherent + 1 normal extrinsic + 1 terminal
            assert_eq!(proposal.block.extrinsics().len(), 3);

            // Importing block #1
            block_on(client.import(BlockOrigin::Own, proposal.block.clone())).unwrap();
        }
    );

    // Both mandatory extrinsics should have been discarded, therefore there are only 3 txs
    // in the block: 1 timestamp inherent + 1 normal extrinsic + 1 terminal
    assert_eq!(proposal.block.extrinsics().len(), 3);

    // Importing block #1
    block_on(client.import(BlockOrigin::Own, proposal.block.clone())).unwrap();

    let best_hash = client.info().best_hash;
    assert_eq!(best_hash, proposal.block.hash());
}

#[test]
fn block_builder_cloned_ok() {
    init_logger();

    let client_builder =
        TestClientBuilder::new().set_execution_strategy(ExecutionStrategy::NativeWhenPossible);
    let backend = client_builder.backend();
    let client = Arc::new(client_builder.build());

    let genesis_hash =
        <[u8; 32]>::try_from(&client.info().best_hash[..]).expect("H256 is a 32 byte type");

    let extrinsics = sign_extrinsics(
        checked_extrinsics(5, bob(), 0, || CallBuilder::noop().build()),
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );

    let mut block_builder = BlockBuilder::new(
        client.as_ref(),
        genesis_hash.into(),
        0_u32,
        false.into(),
        pre_digest(1, 0),
        backend.as_ref(),
    )
    .unwrap();

    extrinsics.into_iter().for_each(|xt: OpaqueExtrinsic| {
        assert_ok!(block_builder.push(xt));
    });

    assert_eq!(block_builder.extrinsics().len(), 5);

    // At this point the overlay wrapped in the `Api` instance has some changes
    let fresh_block_builder = BlockBuilder::new(
        client.as_ref(),
        genesis_hash.into(),
        0_u32,
        false.into(),
        pre_digest(1, 0),
        backend.as_ref(),
    )
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
    let block_builder = BlockBuilder::<'_, _, Client<_, _, _, RA>, _>::from_parts(
        ext_1, api_1, ver_1, phash_1, bd_1, hsize_1,
    );
    let cloned_block_builder = BlockBuilder::<'_, _, Client<_, _, _, RA>, _>::from_parts(
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

#[test]
fn proposal_timing_consistent() {
    use sp_state_machine::IterArgs;

    init_logger();

    init!(client, backend, txpool, spawner, genesis_hash);

    // Create an extrinsic that prefunds the bank account
    let pre_fund_bank_xt = CheckedExtrinsic {
        signed: Some((alice(), signed_extra(0))),
        function: CallBuilder::deposit_to_bank().build(),
    };
    let mut checked = vec![pre_fund_bank_xt];

    // Creating a bunch of extrinsics to use up the quota for txpool processing
    // so that about 100 time-consuming init messages should end up in the queue.
    // It's possible though that not all of them make it into the block - it can depend on a
    // number of factors (timer on the target machine, log level, etc).
    checked.extend(
        checked_extrinsics(100, bob(), 0, || {
            // TODO: this is a "hand-wavy" workaround to have a long-running init message.
            // Should be replaced with a more reliable solution (like zero-cost syscalls
            // in init message that would guarantee incorrect gas estimation)
            CallBuilder::long_init(500_u64).build()
        })
        .into_iter(),
    );
    let extrinsics = sign_extrinsics(
        checked,
        VERSION.spec_version,
        VERSION.transaction_version,
        genesis_hash,
    );

    let block_id = BlockId::number(0);
    let timestamp = Timestamp::current();
    let max_gas = None;
    // Simulate the situation when the `Gear::run()` takes longer time to execute than forecasted
    // (for instance, if the gas metering is not quite precise etc.) by setting the deadline to a
    // smaller value than in reality. On Vara the `max_duration` is 1.5s (which is then transformed
    // into 1s inside the `Proposer` and corresponds to 10^12 `max_block` weight).
    // Here we set it to 0.25s to try hit the timeout during the queue processing.
    let max_duration = 250_u64;

    propose_block!(
        client,
        backend,
        txpool,
        spawner,
        best_hash,
        block_id,
        extrinsics,
        timestamp,
        max_duration,
        max_gas,
        proposal,
        {
            let num_ready_0 = txpool.ready().count();
        },
        {
            // Importing block #1
            block_on(client.import(BlockOrigin::Own, proposal.block.clone())).unwrap();

            let state = backend.state_at(best_hash).unwrap();

            // Check that the message queue has all messages pushed to it
            let queue_entry_prefix = storage_prefix(
                pallet_gear_messenger::Pallet::<Runtime>::name().as_bytes(),
                "Dispatches".as_bytes(),
            );
            let mut queue_entry_args = IterArgs::default();
            queue_entry_args.prefix = Some(&queue_entry_prefix);

            let queue_len = state.keys(queue_entry_args).unwrap().count();

            // Draining tx pool in preparation for block #2
            let best_block_id = BlockId::Hash(best_hash);
            block_on(
                txpool.maintain(chain_event(
                    client
                        .header(client.block_hash_from_id(&best_block_id).unwrap().unwrap())
                        .expect("header get error")
                        .expect("there should be header"),
                )),
            );

            let num_ready_1 = txpool.ready().count();

            // `-1` for the bank account pre-funding which did't put anything in the queue.
            let num_messages = num_ready_0 - num_ready_1 - 1;

            // We expect the `Gear::run()` to have been dropped, hence the queue should
            // still have all the messages originally pushed to it.
            assert_eq!(queue_len, num_messages);
        }
    );

    // Let the `Gear::run()` thread a little more time to finish
    std::thread::sleep(time::Duration::from_millis(500));

    // In the meantime make sure we can still keep creating blocks
    let block_id = BlockId::Hash(best_hash);
    let timestamp = Timestamp::new(timestamp.as_millis() + SLOT_DURATION);
    // This time we set the deadline to a very high value to ensure that all messages go through.
    let max_duration = 15_000_u64;
    // No new extrinsics are added to the block
    let extrinsics = vec![];

    propose_block!(
        client,
        backend,
        txpool,
        spawner,
        best_hash,
        block_id,
        extrinsics,
        timestamp,
        max_duration,
        max_gas,
        proposal,
        {},
        {
            // Importing block #2
            block_on(client.import(BlockOrigin::Own, proposal.block.clone())).unwrap();

            let state = backend.state_at(best_hash).unwrap();

            let queue_entry_prefix = storage_prefix(
                pallet_gear_messenger::Pallet::<Runtime>::name().as_bytes(),
                "Dispatches".as_bytes(),
            );
            let mut queue_entry_args = IterArgs::default();
            queue_entry_args.prefix = Some(&queue_entry_prefix);

            let queue_len = state.keys(queue_entry_args).unwrap().count();
            assert_eq!(queue_len, 0);
        }
    );
}
