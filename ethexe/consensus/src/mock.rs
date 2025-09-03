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
    Address, AnnounceHash, Digest, ToDigest,
    db::*,
    ecdsa::{PrivateKey, PublicKey, SignedData},
    gear::{BatchCommitment, ChainCommitment, CodeCommitment},
    mock::*,
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

impl Mock for BatchCommitmentValidationRequest {
    type Args = ();

    fn mock(_args: Self::Args) -> Self {
        BatchCommitmentValidationRequest {
            digest: H256::random().0.into(),
            head: Some(AnnounceHash(H256::random())),
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

/// Prepare chain with case:
/// ```txt
/// chain:                  [genesis] <- [block1] <- [block2] <- [block3]
/// transitions:                0           2           2           0
/// codes in queue:             0           0           0           2
/// last_committed_batch:      zero        zero        zero        zero
/// last_committed_announce:  genesis     genesis     genesis     genesis
/// ```
pub fn prepare_chain_for_batch_commitment(db: &Database) -> BatchCommitment {
    let mut chain = BlockChain::mock((4, None));

    let chain_commitment1 = ChainCommitment::mock(chain.block_top_announce(1).announce.hash());
    let chain_commitment2 = ChainCommitment::mock(chain.block_top_announce(2).announce.hash());
    chain.block_top_announce_mut(1).as_computed_mut().outcome =
        chain_commitment1.transitions.clone();
    chain.block_top_announce_mut(2).as_computed_mut().outcome =
        chain_commitment2.transitions.clone();

    let code_commitment1 = CodeCommitment::mock(());
    let code_commitment2 = CodeCommitment::mock(());
    chain.blocks[3].prepared.as_mut().unwrap().codes_queue =
        [code_commitment1.id, code_commitment2.id].into();

    let chain = chain.prepare(db, ());
    db.set_code_valid(code_commitment1.id, code_commitment1.valid);
    db.set_code_valid(code_commitment2.id, code_commitment2.valid);

    BatchCommitment {
        block_hash: chain.blocks[3].hash,
        timestamp: chain.blocks[3].as_synced().header.timestamp,
        previous_batch: Digest::zero(),
        chain_commitment: Some(ChainCommitment {
            transitions: [chain_commitment1.transitions, chain_commitment2.transitions].concat(),
            head_announce: db.top_announce_hash(chain.blocks[3].hash),
        }),
        code_commitments: vec![code_commitment1, code_commitment2],
        validators_commitment: None,
        rewards_commitment: None,
    }
}

pub trait SignerMockExt {
    fn mock_signed_data<T: Mock + ToDigest>(
        &self,
        pub_key: PublicKey,
        args: T::Args,
    ) -> SignedData<T>;
}

impl SignerMockExt for Signer {
    fn mock_signed_data<T: Mock + ToDigest>(
        &self,
        pub_key: PublicKey,
        args: T::Args,
    ) -> SignedData<T> {
        self.signed_data(pub_key, T::mock(args)).unwrap()
    }
}
