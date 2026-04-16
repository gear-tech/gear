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

use crate::BatchCommitmentValidationReply;
use ethexe_common::{
    Address, Announce, BlockHeader, Digest, HashOf, ProtocolTimelines, SimpleBlockData, ToDigest,
    ValidatorsVec,
    db::*,
    ecdsa::{PrivateKey, PublicKey, SignedData, VerifiedData},
    gear::{BatchCommitment, ChainCommitment, CodeCommitment, Message, StateTransition},
    injected::InjectedTransaction,
    mock::{
        AnnounceData, BlockChain, BlockFullData, DBMockExt, MockComputedAnnounceData,
        PreparedBlockData as MockPreparedBlockData, SyncedBlockData, Tap,
    },
};
use ethexe_db::Database;
use gear_core::limited::LimitedVec;
use gprimitives::{ActorId, H256, MessageId};
use gsigner::secp256k1::{Secp256k1SignerExt, Signer};
use std::{collections::VecDeque, vec};

const TEST_ROUTER_ADDRESS: Address = Address([0x42; 20]);
const TEST_GENESIS_HASH: H256 = H256([u8::MAX; 32]);
const TEST_GENESIS_HEIGHT: u32 = 1_000_000;
const TEST_GENESIS_TIMESTAMP: u64 = 1_000_000;
const TEST_SLOT: u64 = 10;

pub fn init_signer_with_keys(amount: u8) -> (Signer, Vec<PrivateKey>, Vec<PublicKey>) {
    let signer = Signer::memory();

    let private_keys: Vec<_> = (0..amount)
        .map(|i| PrivateKey::from_seed([i + 1; 32]).expect("valid seed"))
        .collect();
    let public_keys = private_keys
        .iter()
        .map(|key| signer.import(key.clone()).unwrap())
        .collect();
    (signer, private_keys, public_keys)
}

pub fn test_protocol_timelines() -> ProtocolTimelines {
    ProtocolTimelines {
        genesis_ts: TEST_GENESIS_TIMESTAMP,
        era: TEST_SLOT * 100,
        election: TEST_SLOT * 20,
        slot: TEST_SLOT,
    }
}

pub fn test_protocol_timelines_with_slot(slot: u64) -> ProtocolTimelines {
    ProtocolTimelines {
        slot,
        ..test_protocol_timelines()
    }
}

pub fn test_block_hash(index: u64) -> H256 {
    H256::from_low_u64_be(index).tap_mut(|hash| hash.0[0] = 0x10)
}

pub fn test_simple_block_data(index: u64) -> SimpleBlockData {
    let hash = test_block_hash(index);
    let parent_hash = index
        .checked_sub(1)
        .map(test_block_hash)
        .unwrap_or(TEST_GENESIS_HASH);

    SimpleBlockData {
        hash,
        header: BlockHeader {
            height: TEST_GENESIS_HEIGHT + index as u32,
            timestamp: TEST_GENESIS_TIMESTAMP + index * TEST_SLOT,
            parent_hash,
        },
    }
}

pub fn test_announce(block_hash: H256, parent: HashOf<Announce>) -> Announce {
    Announce {
        block_hash,
        parent,
        gas_allowance: Some(100),
        injected_transactions: vec![],
    }
}

pub fn test_code_commitment(seed: u64) -> CodeCommitment {
    CodeCommitment {
        id: test_block_hash(seed).into(),
        valid: true,
    }
}

pub fn test_state_transition(seed: u64) -> StateTransition {
    StateTransition {
        actor_id: ActorId::from(test_block_hash(seed)),
        new_state_hash: test_block_hash(seed + 1),
        exited: false,
        inheritor: ActorId::from(test_block_hash(seed + 2)),
        value_to_receive: 123,
        value_to_receive_negative_sign: false,
        value_claims: vec![],
        messages: vec![Message {
            id: MessageId::from(test_block_hash(seed + 3)),
            destination: ActorId::from(test_block_hash(seed + 4)),
            payload: format!("message-{seed}").into_bytes(),
            value: 0,
            reply_details: None,
            call: false,
        }],
    }
}

pub fn test_chain_commitment(head_announce: HashOf<Announce>, seed: u64) -> ChainCommitment {
    ChainCommitment {
        transitions: vec![
            test_state_transition(seed),
            test_state_transition(seed + 10),
        ],
        head_announce,
    }
}

pub fn test_batch_commitment(block_hash: H256, seed: u64) -> BatchCommitment {
    BatchCommitment {
        block_hash,
        timestamp: TEST_GENESIS_TIMESTAMP + seed,
        previous_batch: Digest::zero(),
        expiry: 10,
        chain_commitment: Some(test_chain_commitment(HashOf::zero(), seed)),
        code_commitments: vec![
            test_code_commitment(seed + 100),
            test_code_commitment(seed + 200),
        ],
        validators_commitment: None,
        rewards_commitment: None,
    }
}

pub fn test_injected_transaction(
    reference_block: H256,
    destination: ActorId,
) -> InjectedTransaction {
    InjectedTransaction {
        destination,
        payload: LimitedVec::new(),
        value: 0,
        reference_block,
        salt: LimitedVec::try_from(vec![reference_block.to_low_u64_be() as u8; 32])
            .expect("fixed salt length fits"),
    }
}

pub fn test_block_chain(len: u32) -> BlockChain {
    test_block_chain_with_validators(len, Default::default())
}

pub fn test_block_chain_with_validators(len: u32, validators: ValidatorsVec) -> BlockChain {
    let mut blocks: VecDeque<_> = (0..=len)
        .map(|index| {
            let block = test_simple_block_data(index as u64);
            BlockFullData {
                hash: block.hash,
                synced: Some(SyncedBlockData {
                    header: block.header,
                    events: Default::default(),
                }),
                prepared: Some(MockPreparedBlockData {
                    codes_queue: Default::default(),
                    announces: Some(Default::default()),
                    last_committed_batch: Digest::zero(),
                    last_committed_announce: HashOf::zero(),
                }),
            }
        })
        .collect();

    let mut genesis_announce_hash = None;
    let mut parent_announce_hash = HashOf::zero();
    let announces = blocks
        .iter_mut()
        .map(|block| {
            let announce = Announce::base(block.hash, parent_announce_hash);
            let announce_hash = announce.to_hash();
            let genesis_announce_hash = genesis_announce_hash.get_or_insert(announce_hash);

            block
                .as_prepared_mut()
                .announces
                .as_mut()
                .expect("block announces exist")
                .insert(announce_hash);
            block.as_prepared_mut().last_committed_announce = *genesis_announce_hash;
            parent_announce_hash = announce_hash;

            (
                announce_hash,
                AnnounceData {
                    announce,
                    computed: Some(MockComputedAnnounceData::default()),
                },
            )
        })
        .collect();

    let config = DBConfig {
        version: 0,
        chain_id: 0,
        router_address: TEST_ROUTER_ADDRESS,
        timelines: test_protocol_timelines(),
        genesis_block_hash: blocks[0].hash,
        genesis_announce_hash: genesis_announce_hash.expect("genesis announce exists"),
    };

    let globals = DBGlobals {
        start_block_hash: blocks[0].hash,
        start_announce_hash: genesis_announce_hash.expect("genesis announce exists"),
        latest_synced_block: blocks.back().expect("chain has blocks").to_simple(),
        latest_prepared_block_hash: blocks.back().expect("chain has blocks").hash,
        latest_computed_announce_hash: parent_announce_hash,
    };

    BlockChain {
        blocks,
        announces,
        codes: Default::default(),
        validators,
        config,
        globals,
    }
}

/// Prepare chain with case:
/// ```txt
/// chain:                  [genesis] <- [block1] <- [block2] <- [block3]
/// transitions:                0           2           2           0
/// codes in queue:             0           0           0           2
/// last_committed_batch:      zero        zero        zero        zero
/// last_committed_announce:  genesis     genesis     genesis     genesis
/// ```
pub fn prepare_chain_for_batch_commitment(db: &Database) -> BatchCommitment {
    let mut chain = test_block_chain(3);

    let transitions1 = vec![test_state_transition(10), test_state_transition(20)];
    let transitions2 = vec![test_state_transition(30), test_state_transition(40)];

    let announce1_hash = chain.block_top_announce_mutate(1, |data| {
        data.announce.gas_allowance = Some(19);
        data.as_computed_mut().outcome = transitions1.clone();
    });

    let announce2_hash = chain.block_top_announce_mutate(2, |data| {
        data.announce.gas_allowance = Some(20);
        data.announce.parent = announce1_hash;
        data.as_computed_mut().outcome = transitions2.clone();
    });

    let announce3_hash = chain.block_top_announce_mutate(3, |data| {
        data.announce.gas_allowance = Some(21);
        data.announce.parent = announce2_hash;
    });

    let code_commitment1 = test_code_commitment(100);
    let code_commitment2 = test_code_commitment(200);
    chain.blocks[3].prepared.as_mut().unwrap().codes_queue =
        [code_commitment1.id, code_commitment2.id].into();

    chain.globals.latest_computed_announce_hash = announce3_hash;

    let block3 = chain.setup(db).blocks[3].to_simple();

    // NOTE: we skipped codes instrumented data in `chain`, so mark them as valid manually,
    // but instrumented data is still not in db.
    db.set_code_valid(code_commitment1.id, code_commitment1.valid);
    db.set_code_valid(code_commitment2.id, code_commitment2.valid);

    BatchCommitment {
        block_hash: block3.hash,
        timestamp: block3.header.timestamp,
        previous_batch: Digest::zero(),
        expiry: 0,
        chain_commitment: Some(ChainCommitment {
            transitions: [transitions1, transitions2].concat(),
            head_announce: db.top_announce_hash(block3.hash),
        }),
        code_commitments: vec![code_commitment1, code_commitment2],
        validators_commitment: None,
        rewards_commitment: None,
    }
}

pub trait SignerMockExt {
    fn signed_test_data<M: ToDigest>(&self, pub_key: PublicKey, message: M) -> SignedData<M>;

    fn verified_test_data<M: ToDigest>(&self, pub_key: PublicKey, message: M) -> VerifiedData<M> {
        self.signed_test_data(pub_key, message).into_verified()
    }

    fn validation_reply(
        &self,
        pub_key: PublicKey,
        contract_address: Address,
        digest: Digest,
    ) -> BatchCommitmentValidationReply;
}

impl SignerMockExt for Signer {
    fn signed_test_data<M: ToDigest>(&self, pub_key: PublicKey, message: M) -> SignedData<M> {
        self.signed_data(pub_key, message, None).unwrap()
    }

    fn validation_reply(
        &self,
        public_key: PublicKey,
        contract_address: Address,
        digest: Digest,
    ) -> BatchCommitmentValidationReply {
        BatchCommitmentValidationReply {
            digest,
            signature: self
                .sign_for_contract_digest(contract_address, public_key, digest, None)
                .unwrap(),
        }
    }
}
