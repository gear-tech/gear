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

use ethexe_common::{
    Address, Digest, ToDigest,
    consensus::BatchCommitmentValidationReply,
    db::CodesStorageRW,
    ecdsa::{PrivateKey, PublicKey, SignedData, VerifiedData},
    gear::{BatchCommitment, ChainCommitment, CodeCommitment},
    mock::{BlockChain, DBMockExt, Mock},
};
use ethexe_db::Database;
use ethexe_signer::Signer;

pub fn init_signer_with_keys(amount: u8) -> (Signer, Vec<PrivateKey>, Vec<PublicKey>) {
    let signer = Signer::memory();

    let private_keys: Vec<_> = (0..amount).map(|i| PrivateKey::from([i + 1; 32])).collect();
    let public_keys = private_keys
        .iter()
        .map(|&key| signer.storage_mut().add_key(key).unwrap())
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

    let chain_commitment1 = ChainCommitment::mock(chain.block_top_announce(1).announce.to_hash());
    let chain_commitment2 = ChainCommitment::mock(chain.block_top_announce(2).announce.to_hash());
    chain.block_top_announce_mut(1).as_computed_mut().outcome =
        chain_commitment1.transitions.clone();
    chain.block_top_announce_mut(2).as_computed_mut().outcome =
        chain_commitment2.transitions.clone();

    let code_commitment1 = CodeCommitment::mock(());
    let code_commitment2 = CodeCommitment::mock(());
    chain.blocks[3].prepared.as_mut().unwrap().codes_queue =
        [code_commitment1.id, code_commitment2.id].into();

    let block3 = chain.setup(db).blocks[3].to_simple();

    // NOTE: we skipped codes instrumented data in `chain`, so mark them as valid manually,
    // but instrumented data is still not in db.
    db.set_code_valid(code_commitment1.id, code_commitment1.valid);
    db.set_code_valid(code_commitment2.id, code_commitment2.valid);

    BatchCommitment {
        block_hash: block3.hash,
        timestamp: block3.header.timestamp,
        previous_batch: Digest::zero(),
        chain_commitment: Some(ChainCommitment {
            transitions: [chain_commitment1.transitions, chain_commitment2.transitions].concat(),
            head_announce: db.top_announce_hash(block3.hash),
        }),
        code_commitments: vec![code_commitment1, code_commitment2],
        validators_commitment: None,
        rewards_commitment: None,
    }
}

pub trait SignerMockExt {
    fn mock_signed_data<M, Args>(&self, pub_key: PublicKey, args: Args) -> SignedData<M>
    where
        M: Mock<Args> + ToDigest;

    fn mock_verified_data<M, Args>(&self, pub_key: PublicKey, args: Args) -> VerifiedData<M>
    where
        M: Mock<Args> + ToDigest,
    {
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
    fn mock_signed_data<M, Args>(&self, pub_key: PublicKey, args: Args) -> SignedData<M>
    where
        M: Mock<Args> + ToDigest,
    {
        self.signed_data(pub_key, M::mock(args)).unwrap()
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
                .sign_for_contract(contract_address, public_key, digest)
                .unwrap(),
        }
    }
}
