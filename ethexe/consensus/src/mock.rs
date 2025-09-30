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

use crate::{BatchCommitmentValidationReply, BatchCommitmentValidationRequest};
use ethexe_common::{
    Address, Announce, AnnounceHash, BlockHeader, Digest, SimpleBlockData, ToDigest,
    db::*,
    ecdsa::{PrivateKey, PublicKey, SignedData},
    gear::{BatchCommitment, ChainCommitment, CodeCommitment, Message, StateTransition},
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
    type Args = H256;

    fn mock(parent: H256) -> Self {
        SimpleBlockData {
            hash: H256::random(),
            header: BlockHeader {
                height: 43,
                timestamp: 120,
                parent_hash: parent,
            },
        }
    }
}

impl Mock for Announce {
    type Args = (H256, AnnounceHash);

    fn mock((block_hash, parent): (H256, AnnounceHash)) -> Self {
        Announce {
            block_hash,
            parent,
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
            head: Some(AnnounceHash(H256::random())),
            codes: vec![CodeCommitment::mock(()).id, CodeCommitment::mock(()).id],
            validators: false,
            rewards: false,
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
    type Args = AnnounceHash;

    fn mock(head_announce: Self::Args) -> Self {
        ChainCommitment {
            transitions: vec![StateTransition::mock(()), StateTransition::mock(())],
            head_announce,
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
            chain_commitment: Some(ChainCommitment::mock(AnnounceHash::random())),
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
    type Args = AnnounceHash;

    fn prepare(self, db: &Database, last_committed_announce: AnnounceHash) -> Self {
        db.set_block_header(self.hash, self.header);

        let parent_announce = db
            .block_meta(self.header.parent_hash)
            .announces
            .map(|a| a[0])
            .unwrap_or(last_committed_announce);
        let announce = Announce::mock((self.hash, parent_announce));
        let announce_hash = db.set_announce(announce);
        db.set_announce_outcome(announce_hash, Default::default());
        db.mutate_announce_meta(announce_hash, |meta| {
            *meta = AnnounceMeta { computed: true }
        });

        db.mutate_block_meta(self.hash, |meta| {
            *meta = BlockMeta {
                prepared: true,
                announces: Some(vec![announce_hash]),
                codes_queue: Some(Default::default()),
                last_committed_batch: None,
                last_committed_announce: Some(last_committed_announce),
            }
        });

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
    type Args = ();

    fn prepare(self, db: &Database, _args: ()) -> Self {
        let Self {
            transitions,
            head_announce: head,
        } = &self;
        db.set_announce_outcome(*head, transitions.clone());
        self
    }
}

pub fn prepared_mock_batch_commitment(db: &Database) -> BatchCommitment {
    // [block3] <- [block2] <- [block1] <- [block0]

    let block3 = SimpleBlockData::mock(H256::zero()).prepare(db, AnnounceHash::random());
    let block3_announce_hash = db.block_meta(block3.hash).announces.map(|a| a[0]).unwrap();

    let block2 = SimpleBlockData::mock(block3.hash).prepare(db, block3_announce_hash);
    let block1 = SimpleBlockData::mock(block2.hash).prepare(db, block3_announce_hash);
    let block0 = SimpleBlockData::mock(block1.hash).prepare(db, block3_announce_hash);

    let last_committed_batch = Digest::random();
    db.mutate_block_meta(block0.hash, |meta| {
        meta.last_committed_batch = Some(last_committed_batch);
    });

    let cc1 =
        ChainCommitment::mock(db.block_meta(block1.hash).announces.unwrap()[0]).prepare(db, ());
    let cc2 =
        ChainCommitment::mock(db.block_meta(block2.hash).announces.unwrap()[0]).prepare(db, ());

    let code_commitment1 = CodeCommitment::mock(()).prepare(db, ());
    let code_commitment2 = CodeCommitment::mock(()).prepare(db, ());
    db.mutate_block_meta(block0.hash, |m| {
        m.codes_queue = Some(From::from([code_commitment1.id, code_commitment2.id]))
    });

    BatchCommitment {
        block_hash: block0.hash,
        timestamp: block0.header.timestamp,
        previous_batch: last_committed_batch,
        chain_commitment: Some(ChainCommitment {
            transitions: [cc2.transitions, cc1.transitions].concat(),
            head_announce: db.block_meta(block0.hash).announces.unwrap()[0],
        }),
        code_commitments: vec![code_commitment1, code_commitment2],
        validators_commitment: None,
        rewards_commitment: None,
    }
}

pub trait DBExt {
    fn simple_block_data(&self, block: H256) -> SimpleBlockData;
    fn announce_hash(&self, block: H256) -> AnnounceHash;
}

impl DBExt for Database {
    fn simple_block_data(&self, block: H256) -> SimpleBlockData {
        let header = self.block_header(block).expect("block header not found");
        SimpleBlockData {
            hash: block,
            header,
        }
    }

    fn announce_hash(&self, block: H256) -> AnnounceHash {
        self.block_meta(block)
            .announces
            .expect("block announces not found")
            .into_iter()
            .next()
            .expect("must be at list one announce")
    }
}
