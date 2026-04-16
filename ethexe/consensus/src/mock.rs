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
    Address, Digest, ToDigest,
    db::*,
    ecdsa::{PrivateKey, PublicKey, SignedData, VerifiedData},
    gear::{BatchCommitment, ChainCommitment, CodeCommitment, StateTransition},
    mock::*,
};
use ethexe_db::Database;
use gsigner::secp256k1::{Secp256k1SignerExt, Signer};
use std::vec;

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

/// Prepare chain with case:
/// ```txt
/// chain:                  [genesis] <- [block1] <- [block2] <- [block3]
/// transitions:                0           2           2           0
/// codes in queue:             0           0           0           2
/// last_committed_batch:      zero        zero        zero        zero
/// last_committed_announce:  genesis     genesis     genesis     genesis
/// ```
pub fn prepare_chain_for_batch_commitment(db: &Database) -> BatchCommitment {
    let mut chain = BlockChain::mock(3);

    let transitions1 = vec![StateTransition::mock(()), StateTransition::mock(())];
    let transitions2 = vec![StateTransition::mock(()), StateTransition::mock(())];

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

    let code_commitment1 = CodeCommitment::mock(());
    let code_commitment2 = CodeCommitment::mock(());
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
    fn mock_signed_data<T, M: Mock<T> + ToDigest>(
        &self,
        pub_key: PublicKey,
        args: T,
    ) -> SignedData<M>;

    fn mock_verified_data<T, M: Mock<T> + ToDigest>(
        &self,
        pub_key: PublicKey,
        args: T,
    ) -> VerifiedData<M> {
        self.mock_signed_data(pub_key, args).into_verified()
    }

    fn validation_reply(
        &self,
        pub_key: PublicKey,
        contract_address: Address,
        digest: Digest,
    ) -> BatchCommitmentValidationReply;
}

impl SignerMockExt for Signer {
    fn mock_signed_data<T, M: Mock<T> + ToDigest>(
        &self,
        pub_key: PublicKey,
        args: T,
    ) -> SignedData<M> {
        self.signed_data(pub_key, M::mock(args), None).unwrap()
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
