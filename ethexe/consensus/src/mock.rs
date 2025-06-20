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

use crate::{utils, BatchCommitmentValidationReply, BatchCommitmentValidationRequest};
use ethexe_common::{
    db::{BlockMetaStorageWrite, CodesStorageWrite, OnChainStorageWrite},
    ecdsa::{PrivateKey, PublicKey, SignedData},
    gear::{BatchCommitment, ChainCommitment, CodeCommitment, GearBlock, Message, StateTransition},
    Address, BlockHeader, Digest, ProducerBlock, SimpleBlockData, ToDigest,
};
use ethexe_db::Database;
use ethexe_signer::Signer;
use gprimitives::H256;
use std::vec;

pub fn init_signer_with_keys(amount: u8) -> (Signer, Vec<PrivateKey>, Vec<PublicKey>) {
    let signer = Signer::memory();

    let private_keys: Vec<_> = (0..amount).map(|i| PrivateKey::from([i + 1; 32])).collect();
    let public_keys = private_keys
        .iter()
        .map(|&key| signer.storage_mut().add_key(key).unwrap())
        .collect();
    (signer, private_keys, public_keys)
}

pub trait Mock {
    type Args;

    fn mock(args: Self::Args) -> Self;
}

impl<T: Mock + ToDigest> Mock for SignedData<T> {
    type Args = (Signer, PublicKey, T::Args);

    fn mock((signer, public_key, args): Self::Args) -> Self {
        signer.signed_data(public_key, T::mock(args)).unwrap()
    }
}

impl Mock for SimpleBlockData {
    type Args = ();

    fn mock(_args: Self::Args) -> Self {
        SimpleBlockData {
            hash: H256::random(),
            header: BlockHeader {
                height: 43,
                timestamp: 120,
                parent_hash: H256::random(),
            },
        }
    }
}

impl Mock for ProducerBlock {
    type Args = H256;

    fn mock(block_hash: H256) -> Self {
        ProducerBlock {
            block_hash,
            gas_allowance: Some(100),
            off_chain_transactions: vec![],
        }
    }
}

impl Mock for BatchCommitmentValidationRequest {
    type Args = ();

    fn mock(_args: Self::Args) -> Self {
        BatchCommitmentValidationRequest {
            digest: H256::random().0.into(),
            blocks: vec![H256::random(), H256::random()],
            codes: vec![CodeCommitment::mock(()).id, CodeCommitment::mock(()).id],
        }
    }
}

impl Mock for BatchCommitmentValidationReply {
    type Args = (Signer, PublicKey, Address, Digest);

    fn mock((signer, public_key, contract_address, digest): Self::Args) -> Self {
        BatchCommitmentValidationReply {
            digest,
            signature: signer
                .sign_for_contract(contract_address, public_key, digest)
                .unwrap(),
        }
    }
}

impl Mock for CodeCommitment {
    type Args = ();

    fn mock(_args: Self::Args) -> Self {
        CodeCommitment {
            id: H256::random().into(),
            valid: true,
        }
    }
}

impl Mock for ChainCommitment {
    type Args = H256;

    fn mock(block_hash: Self::Args) -> Self {
        ChainCommitment {
            transitions: vec![StateTransition::mock(()), StateTransition::mock(())],
            gear_blocks: vec![GearBlock {
                hash: block_hash,
                gas_allowance: 0,
                off_chain_transactions_hash: H256::zero(),
            }],
        }
    }
}

impl Mock for BatchCommitment {
    type Args = ();

    fn mock(_args: Self::Args) -> Self {
        BatchCommitment {
            block_hash: H256::random(),
            timestamp: 42,
            previous_batch: Digest::random(),
            chain_commitment: Some(ChainCommitment::mock(H256::random())),
            code_commitments: vec![CodeCommitment::mock(()), CodeCommitment::mock(())],
            validators_commitment: None,
            rewards_commitment: None,
        }
    }
}

impl Mock for StateTransition {
    type Args = ();

    fn mock(_args: Self::Args) -> Self {
        StateTransition {
            actor_id: H256::random().into(),
            new_state_hash: H256::random(),
            inheritor: H256::random().into(),
            value_to_receive: 123,
            value_claims: vec![],
            messages: vec![Message {
                id: H256::random().into(),
                destination: H256::random().into(),
                payload: b"Hello, World!".to_vec(),
                value: 0,
                reply_details: None,
                call: false,
            }],
            exited: false,
        }
    }
}

pub trait Prepare {
    type Args;

    fn prepare(self, db: &Database, args: Self::Args) -> Self;
}

impl Prepare for SimpleBlockData {
    type Args = ();

    fn prepare(self, db: &Database, _args: ()) -> Self {
        db.set_block_header(self.hash, self.header.clone());
        db.set_block_computed(self.hash);
        db.set_block_outcome(self.hash, Default::default());
        db.set_block_codes_queue(self.hash, Default::default());
        db.set_block_commitment_queue(self.hash, Default::default());
        self
    }
}

impl Prepare for CodeCommitment {
    type Args = ();

    fn prepare(self, db: &Database, _args: ()) -> Self {
        db.set_code_valid(self.id, self.valid);
        self
    }
}

impl Prepare for ChainCommitment {
    type Args = H256;

    fn prepare(self, db: &Database, previous_not_empty_block: H256) -> Self {
        let Self {
            transitions,
            gear_blocks,
        } = self;

        assert!(gear_blocks.len() == 1, "Only one gear block is supported");

        let block = gear_blocks.into_iter().next().unwrap();

        db.set_block_outcome(block.hash, transitions.clone());
        db.set_previous_not_empty_block(block.hash, previous_not_empty_block);

        Self {
            transitions,
            gear_blocks: vec![block],
        }
    }
}

pub fn prepared_mock_batch_commitment(
    db: &Database,
    chain_head: &SimpleBlockData,
) -> BatchCommitment {
    // ... <- [block2] <- ... <- [block1] <- ... <- [chain_head]
    let block1 = SimpleBlockData::mock(()).prepare(db, ());
    let block2 = SimpleBlockData::mock(()).prepare(db, ());
    let last_committed_batch = Digest::random();

    let chain_commitment1 = ChainCommitment::mock(block1.hash).prepare(db, block2.hash);
    let chain_commitment2 = ChainCommitment::mock(block2.hash).prepare(db, H256::random());
    db.set_block_commitment_queue(chain_head.hash, From::from([block2.hash, block1.hash]));
    db.set_last_committed_batch(chain_head.hash, last_committed_batch);

    let code_commitment1 = CodeCommitment::mock(()).prepare(db, ());
    let code_commitment2 = CodeCommitment::mock(()).prepare(db, ());
    db.set_block_codes_queue(
        chain_head.hash,
        From::from([code_commitment1.id, code_commitment2.id]),
    );

    BatchCommitment {
        block_hash: chain_head.hash,
        timestamp: chain_head.header.timestamp,
        previous_batch: last_committed_batch,
        chain_commitment: utils::squash_chain_commitments(vec![
            chain_commitment2,
            chain_commitment1,
        ]),
        code_commitments: vec![code_commitment1, code_commitment2],
        validators_commitment: None,
        rewards_commitment: None,
    }
}
