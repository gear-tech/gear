// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use crate::authorship::*;

use codec::Encode;
use core::convert::TryFrom;
use frame_support::{storage::storage_prefix, traits::PalletInfoAccess};
use futures::executor::block_on;
use sc_client_api::Backend;
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
    Digest, DigestItem,
};
use std::{sync::Arc, thread::sleep, time};
use testing::{
    client::{ClientBlockImportExt, TestClientBuilder, TestClientBuilderExt},
    keyring::{alice, bob, sign, signed_extra, CheckedExtrinsic},
};
use vara_runtime::{AccountId, Runtime, RuntimeCall, SLOT_DURATION, VERSION};

const SOURCE: TransactionSource = TransactionSource::External;

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

fn checked_extrinsics(n: u32, signer: AccountId, nonce: &mut u32) -> Vec<CheckedExtrinsic> {
    use demo_mul_by_const::WASM_BINARY;
    use std::fmt::Write;

    (0..n)
        .map(|i| {
            let mut salt = String::new();
            write!(salt, "salt_{}", *nonce).expect("Failure writing to String");
            let tx = CheckedExtrinsic {
                signed: Some((signer.clone(), signed_extra(*nonce))),
                function: RuntimeCall::Gear(pallet_gear::Call::upload_program {
                    code: WASM_BINARY.to_vec(),
                    salt: salt.as_bytes().to_vec(),
                    init_payload: (i as u64).encode(),
                    gas_limit: 10_000_000,
                    value: 0,
                }),
            };
            *nonce += 1;
            tx
        })
        .collect()
}

#[test]
fn custom_extrinsic_is_placed_in_each_block() {
    let client = Arc::new(
        TestClientBuilder::new()
            .set_execution_strategy(sc_client_api::ExecutionStrategy::NativeWhenPossible)
            .build(),
    );
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
    let mut nonce = 0_u32;
    let extrinsics = checked_extrinsics(1, bob(), &mut nonce)
        .iter()
        .map(|x| {
            sign(
                x.clone(),
                VERSION.spec_version,
                VERSION.transaction_version,
                genesis_hash,
            )
            .into()
        })
        .collect::<Vec<_>>();

    block_on(txpool.submit_at(&BlockId::number(0), SOURCE, extrinsics)).unwrap();
    block_on(
        txpool.maintain(chain_event(
            client
                .header(
                    client
                        .block_hash_from_id(&BlockId::Number(0_u32))
                        .unwrap()
                        .unwrap(),
                )
                .expect("get header error")
                .expect("there should be a header"),
        )),
    );
    assert_eq!(txpool.ready().count(), 1);

    let mut proposer_factory = ProposerFactory::new(spawner, client.clone(), txpool, None, None);
    let timestamp_provider = sp_timestamp::InherentDataProvider::from_system_time();
    let time_slot = sp_timestamp::Timestamp::current().as_millis() / SLOT_DURATION;

    let proposer = block_on(
        proposer_factory.init(
            &client
                .header(
                    client
                        .block_hash_from_id(&BlockId::number(0))
                        .unwrap()
                        .unwrap(),
                )
                .expect("Database error querying block #0")
                .expect("Block #0 should exist"),
        ),
    )
    .expect("Proposer initialization failed");

    let inherent_data =
        block_on(timestamp_provider.create_inherent_data()).expect("Create inherent data failed");

    let block = block_on(proposer.propose(
        inherent_data,
        pre_digest(time_slot, 0),
        time::Duration::from_secs(20),
        None,
    ))
    .map(|r| r.block)
    .unwrap();

    // then
    // block should have exactly 3 txs: an inherent (timestamp), a normal and a mandatory one
    assert_eq!(block.extrinsics().len(), 3);
}

#[test]
fn proposed_storage_changes_match_execute_block_storage_changes() {
    let client_builder = TestClientBuilder::new()
        .set_execution_strategy(sc_client_api::ExecutionStrategy::NativeWhenPossible);
    let backend = client_builder.backend();
    let client = Arc::new(client_builder.build());
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
    let mut nonce = 0_u32;
    let extrinsics = checked_extrinsics(1, bob(), &mut nonce)
        .iter()
        .map(|x| {
            sign(
                x.clone(),
                VERSION.spec_version,
                VERSION.transaction_version,
                genesis_hash,
            )
            .into()
        })
        .collect::<Vec<_>>();

    block_on(txpool.submit_at(&BlockId::number(0), SOURCE, extrinsics)).unwrap();

    block_on(
        txpool.maintain(chain_event(
            client
                .header(
                    client
                        .block_hash_from_id(&BlockId::Number(0_u32))
                        .unwrap()
                        .unwrap(),
                )
                .expect("header get error")
                .expect("there should be header"),
        )),
    );

    let mut proposer_factory = ProposerFactory::new(spawner, client.clone(), txpool, None, None);
    let timestamp_provider = sp_timestamp::InherentDataProvider::from_system_time();
    let time_slot = sp_timestamp::Timestamp::current().as_millis() / SLOT_DURATION;

    let proposer = block_on(
        proposer_factory.init(
            &client
                .header(
                    client
                        .block_hash_from_id(&BlockId::number(0))
                        .unwrap()
                        .unwrap(),
                )
                .expect("Database error querying block #0")
                .expect("Block #0 should exist"),
        ),
    )
    .expect("Proposer initialization failed");

    let inherent_data =
        block_on(timestamp_provider.create_inherent_data()).expect("Create inherent data failed");

    let proposal = block_on(proposer.propose(
        inherent_data,
        pre_digest(time_slot, 0),
        time::Duration::from_secs(300),
        None,
    ))
    .unwrap();

    // 1 inherent + 1 signed extrinsic + 1 terminal unsigned one
    assert_eq!(proposal.block.extrinsics().len(), 3);

    let api = client.runtime_api();
    api.execute_block(genesis_hash.into(), proposal.block)
        .unwrap();

    let state = backend.state_at(genesis_hash.into()).unwrap();

    let storage_changes = api
        .into_storage_changes(&state, genesis_hash.into())
        .unwrap();

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

#[test]
#[ignore = "Unstable due time timestamp inconsistency"]
fn queue_remains_intact_if_processing_fails() {
    use sp_state_machine::IterArgs;

    let client_builder = TestClientBuilder::new()
        .set_execution_strategy(sc_client_api::ExecutionStrategy::NativeWhenPossible);
    let backend = client_builder.backend();
    let mut client = Arc::new(client_builder.build());
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
    let mut nonce = 0_u32;
    let mut checked = checked_extrinsics(5, bob(), &mut nonce);
    // Disable queue processing in Gear pallet as the root
    checked.push(CheckedExtrinsic {
        signed: Some((alice(), signed_extra(0))),
        function: RuntimeCall::Sudo(pallet_sudo::Call::sudo {
            call: Box::new(RuntimeCall::Gear(pallet_gear::Call::set_execute_inherent {
                value: false,
            })),
        }),
    });
    let extrinsics = checked
        .iter()
        .map(|x| {
            sign(
                x.clone(),
                VERSION.spec_version,
                VERSION.transaction_version,
                genesis_hash,
            )
            .into()
        })
        .collect::<Vec<_>>();

    block_on(txpool.submit_at(&BlockId::number(0), SOURCE, extrinsics)).unwrap();

    block_on(
        txpool.maintain(chain_event(
            client
                .header(
                    client
                        .block_hash_from_id(&BlockId::Number(0_u32))
                        .unwrap()
                        .unwrap(),
                )
                .expect("header get error")
                .expect("there should be header"),
        )),
    );

    let mut proposer_factory =
        ProposerFactory::new(spawner, client.clone(), txpool.clone(), None, None);
    let timestamp_provider = sp_timestamp::InherentDataProvider::from_system_time();

    let proposer = block_on(
        proposer_factory.init(
            &client
                .header(
                    client
                        .block_hash_from_id(&BlockId::number(0))
                        .unwrap()
                        .unwrap(),
                )
                .expect("Database error querying block #0")
                .expect("Block #0 should exist"),
        ),
    )
    .expect("Proposer initialization failed");

    let inherent_data =
        block_on(timestamp_provider.create_inherent_data()).expect("Create inherent data failed");
    let time_slot = sp_timestamp::Timestamp::current().as_millis() / SLOT_DURATION;

    let proposal = block_on(proposer.propose(
        inherent_data,
        pre_digest(time_slot, 0),
        time::Duration::from_secs(20),
        None,
    ))
    .unwrap();

    // Terminal extrinsic rolled back, therefore only have 1 inherent + 6 normal
    assert_eq!(proposal.block.extrinsics().len(), 7);

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

    let best_block_id = BlockId::Hash(best_hash);

    // Preparing block #2
    let extrinsics = checked_extrinsics(3, bob(), &mut nonce)
        .iter()
        .map(|x| {
            sign(
                x.clone(),
                VERSION.spec_version,
                VERSION.transaction_version,
                genesis_hash,
            )
            .into()
        })
        .collect::<Vec<_>>();

    // Pushing 3 more signed extrinsics that add 3 more messages to the queue
    block_on(txpool.submit_at(&BlockId::number(0), SOURCE, extrinsics)).unwrap();

    block_on(
        txpool.maintain(chain_event(
            client
                .header(client.block_hash_from_id(&best_block_id).unwrap().unwrap())
                .expect("header get error")
                .expect("there should be header"),
        )),
    );

    // Wait for a while until the next produced time_slot likely has a higher number
    sleep(time::Duration::from_millis(SLOT_DURATION / 2));

    let proposer = block_on(
        proposer_factory.init(
            &client
                .header(client.block_hash_from_id(&best_block_id).unwrap().unwrap())
                .expect("Database error querying block #1")
                .expect("Block #1 should exist"),
        ),
    )
    .expect("Proposer initialization failed");

    let inherent_data =
        block_on(timestamp_provider.create_inherent_data()).expect("Create inherent data failed");
    let time_slot = sp_timestamp::Timestamp::current().as_millis() / SLOT_DURATION;

    let proposal = block_on(proposer.propose(
        inherent_data,
        pre_digest(time_slot, 0),
        time::Duration::from_secs(20),
        None,
    ))
    .unwrap();

    // Terminal extrinsic rolled back, therefore only have 1 inherent + 3 normal
    assert_eq!(proposal.block.extrinsics().len(), 4);

    // Importing block #2
    block_on(client.import(BlockOrigin::Own, proposal.block.clone())).unwrap();

    let best_hash = client.info().best_hash;
    assert_eq!(best_hash, proposal.block.hash());

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
