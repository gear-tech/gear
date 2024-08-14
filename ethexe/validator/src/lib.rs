// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage},
    router::{BlockCommitment, CodeCommitment},
};
use ethexe_sequencer::agro::{self, AggregatedCommitments};
use ethexe_signer::{Address, AsDigest, Digest, PublicKey, Signature, Signer};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::ops::Not;

pub struct Validator {
    pub_key: PublicKey,
    signer: Signer,
    router_address: Address,
}

pub struct Config {
    pub pub_key: PublicKey,
    pub router_address: Address,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct BlockCommitmentValidationRequest {
    pub block_hash: H256,
    pub allowed_pred_block_hash: H256,
    pub allowed_prev_commitment_hash: H256,
    pub transitions_digest: Digest,
}

impl From<&BlockCommitment> for BlockCommitmentValidationRequest {
    fn from(commitment: &BlockCommitment) -> Self {
        Self {
            block_hash: commitment.block_hash,
            allowed_pred_block_hash: commitment.pred_block_hash,
            allowed_prev_commitment_hash: commitment.prev_commitment_hash,
            transitions_digest: commitment.transitions.as_digest(),
        }
    }
}

impl AsDigest for BlockCommitmentValidationRequest {
    fn as_digest(&self) -> Digest {
        let mut message = Vec::with_capacity(3 * size_of::<H256>() + size_of::<Digest>());

        message.extend_from_slice(self.block_hash.as_bytes());
        message.extend_from_slice(self.allowed_pred_block_hash.as_bytes());
        message.extend_from_slice(self.allowed_prev_commitment_hash.as_bytes());
        message.extend_from_slice(self.transitions_digest.as_ref());

        message.as_digest()
    }
}

impl Validator {
    pub fn new(config: &Config, signer: Signer) -> Self {
        Self {
            signer,
            pub_key: config.pub_key,
            router_address: config.router_address,
        }
    }

    pub fn pub_key(&self) -> PublicKey {
        self.pub_key
    }

    pub fn address(&self) -> Address {
        self.pub_key.to_address()
    }

    pub fn aggregate<C: AsDigest>(&self, commitments: Vec<C>) -> Result<AggregatedCommitments<C>> {
        AggregatedCommitments::aggregate_commitments(
            commitments,
            &self.signer,
            self.pub_key,
            self.router_address,
        )
    }

    pub fn validate_code_commitments(
        &mut self,
        db: &impl CodesStorage,
        requests: impl IntoIterator<Item = CodeCommitment>,
    ) -> Result<Signature> {
        let mut commitment_digests = Vec::new();
        for request in requests.into_iter() {
            commitment_digests.push(request.as_digest());
            Self::validate_code_commitment(db, request)?;
        }

        let commitments_digest = commitment_digests.as_digest();
        agro::sign_commitments_digest(
            commitments_digest,
            &self.signer,
            self.pub_key,
            self.router_address,
        )
    }

    pub fn validate_block_commitments(
        &mut self,
        db: &impl BlockMetaStorage,
        requests: impl IntoIterator<Item = BlockCommitmentValidationRequest>,
    ) -> Result<Signature> {
        let mut commitment_digests = Vec::new();
        for request in requests.into_iter() {
            commitment_digests.push(request.as_digest());
            Self::validate_block_commitment(db, request)?;
        }

        let commitments_digest = commitment_digests.as_digest();
        agro::sign_commitments_digest(
            commitments_digest,
            &self.signer,
            self.pub_key,
            self.router_address,
        )
    }

    fn validate_code_commitment(db: &impl CodesStorage, request: CodeCommitment) -> Result<()> {
        let CodeCommitment { id: code_id, valid } = request;
        if db
            .code_approved(code_id)
            .ok_or(anyhow!("Code {code_id} is not processed by this node"))?
            .ne(&valid)
        {
            return Err(anyhow!("Requested and local code approval mismatch"));
        }
        Ok(())
    }

    fn validate_block_commitment(
        db: &impl BlockMetaStorage,
        request: BlockCommitmentValidationRequest,
    ) -> Result<()> {
        let BlockCommitmentValidationRequest {
            block_hash,
            allowed_pred_block_hash,
            allowed_prev_commitment_hash,
            transitions_digest,
        } = request;

        if db
            .block_end_state_is_valid(block_hash)
            .unwrap_or(false)
            .not()
        {
            return Err(anyhow!(
                "Requested block {block_hash} is not processed by this node"
            ));
        }

        if db
            .block_outcome(block_hash)
            .ok_or(anyhow!("Cannot get from db outcome for block {block_hash}"))?
            .iter()
            .map(AsDigest::as_digest)
            .collect::<Vec<_>>()
            .as_digest()
            .ne(&transitions_digest)
        {
            return Err(anyhow!("Requested and local transitions digest mismatch"));
        }

        if db
            .block_prev_commitment(block_hash)
            .ok_or(anyhow!(
                "Cannot get from db previous commitment block for block {block_hash}"
            ))?
            .ne(&allowed_prev_commitment_hash)
        {
            return Err(anyhow!(
                "Requested and local previous commitment block hash mismatch"
            ));
        }

        if Self::verify_is_predecessor(db, allowed_pred_block_hash, block_hash, None)?.not() {
            return Err(anyhow!(
                "{block_hash} is not a predecessor of {allowed_pred_block_hash}"
            ));
        }

        Ok(())
    }

    /// Verify whether `pred_hash` is a predecessor of `block_hash` in the chain.
    fn verify_is_predecessor(
        db: &impl BlockMetaStorage,
        block_hash: H256,
        pred_hash: H256,
        max_distance: Option<u32>,
    ) -> Result<bool> {
        if block_hash == pred_hash {
            return Ok(true);
        }

        let block_header = db
            .block_header(block_hash)
            .ok_or(anyhow!("header not found for block: {block_hash}"))?;

        if block_header.parent_hash == pred_hash {
            return Ok(true);
        }

        let pred_height = db
            .block_header(pred_hash)
            .ok_or(anyhow!("header not found for pred block: {pred_hash}"))?
            .height;

        let distance = block_header.height.saturating_sub(pred_height);
        if max_distance.map(|d| d < distance).unwrap_or(false) {
            return Err(anyhow!("distance is too large: {distance}"));
        }

        let mut block_hash = block_hash;
        for _ in 0..=distance {
            if block_hash == pred_hash {
                return Ok(true);
            }
            block_hash = db
                .block_header(block_hash)
                .ok_or(anyhow!("header not found for block: {block_hash}"))?
                .parent_hash;
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_db::BlockHeader;
    use gprimitives::CodeId;

    #[test]
    fn test_validate_code_commitments() {
        let db = ethexe_db::Database::from_one(&ethexe_db::MemDb::default(), [0; 20]);

        let code_id = CodeId::from(H256::random());

        Validator::validate_code_commitment(
            &db,
            CodeCommitment {
                id: code_id,
                valid: true,
            },
        )
        .expect_err("Code is not in db");

        db.set_code_approved(code_id, true);
        Validator::validate_code_commitment(
            &db,
            CodeCommitment {
                id: code_id,
                valid: false,
            },
        )
        .expect_err("Approval mismatch");

        Validator::validate_code_commitment(
            &db,
            CodeCommitment {
                id: code_id,
                valid: true,
            },
        )
        .unwrap();
    }

    #[test]
    fn test_validate_block_commitment() {
        let db = ethexe_db::Database::from_one(&ethexe_db::MemDb::default(), [0; 20]);

        let block_hash = H256::random();
        let pred_block_hash = H256::random();
        let prev_commitment_hash = H256::random();
        let transitions = vec![];
        let transitions_digest = transitions.as_digest();

        db.set_block_end_state_is_valid(block_hash, true);
        db.set_block_outcome(block_hash, transitions);
        db.set_block_prev_commitment(block_hash, prev_commitment_hash);
        db.set_block_header(
            block_hash,
            BlockHeader {
                height: 100,
                timestamp: 100,
                parent_hash: pred_block_hash,
            },
        );

        Validator::validate_block_commitment(
            &db,
            BlockCommitmentValidationRequest {
                block_hash,
                allowed_pred_block_hash: block_hash,
                allowed_prev_commitment_hash: prev_commitment_hash,
                transitions_digest,
            },
        )
        .unwrap();

        Validator::validate_block_commitment(
            &db,
            BlockCommitmentValidationRequest {
                block_hash,
                allowed_pred_block_hash: H256::random(),
                allowed_prev_commitment_hash: prev_commitment_hash,
                transitions_digest,
            },
        )
        .expect_err("Unknown pred block is provided");

        Validator::validate_block_commitment(
            &db,
            BlockCommitmentValidationRequest {
                block_hash,
                allowed_pred_block_hash: block_hash,
                allowed_prev_commitment_hash: H256::random(),
                transitions_digest,
            },
        )
        .expect_err("Unknown prev commitment is provided");

        Validator::validate_block_commitment(
            &db,
            BlockCommitmentValidationRequest {
                block_hash,
                allowed_pred_block_hash: block_hash,
                allowed_prev_commitment_hash: prev_commitment_hash,
                transitions_digest: Digest::from([2; 32]),
            },
        )
        .expect_err("Transitions digest mismatch");

        Validator::validate_block_commitment(
            &db,
            BlockCommitmentValidationRequest {
                block_hash: H256::random(),
                allowed_pred_block_hash: block_hash,
                allowed_prev_commitment_hash: prev_commitment_hash,
                transitions_digest,
            },
        )
        .expect_err("Block is not processed by this node");
    }

    #[test]
    fn test_verify_is_predecessor() {
        let db = ethexe_db::Database::from_one(&ethexe_db::MemDb::default(), [0; 20]);

        let blocks = [H256::random(), H256::random(), H256::random()];
        db.set_block_header(
            blocks[0],
            BlockHeader {
                height: 100,
                timestamp: 100,
                parent_hash: H256::zero(),
            },
        );
        db.set_block_header(
            blocks[1],
            BlockHeader {
                height: 101,
                timestamp: 101,
                parent_hash: blocks[0],
            },
        );
        db.set_block_header(
            blocks[2],
            BlockHeader {
                height: 102,
                timestamp: 102,
                parent_hash: blocks[1],
            },
        );

        Validator::verify_is_predecessor(&db, blocks[1], H256::random(), None)
            .expect_err("Unknown pred block is provided");

        Validator::verify_is_predecessor(&db, H256::random(), blocks[0], None)
            .expect_err("Unknown block is provided");

        Validator::verify_is_predecessor(&db, blocks[2], blocks[0], Some(1))
            .expect_err("Distance is too large");

        // Another chain block
        let block3 = H256::random();
        db.set_block_header(
            block3,
            BlockHeader {
                height: 1,
                timestamp: 1,
                parent_hash: blocks[0],
            },
        );
        Validator::verify_is_predecessor(&db, blocks[2], block3, None)
            .expect_err("Block is from other chain with incorrect height");

        assert!(Validator::verify_is_predecessor(&db, blocks[2], blocks[0], None).unwrap());
        assert!(Validator::verify_is_predecessor(&db, blocks[2], blocks[0], Some(2)).unwrap());
        assert!(!Validator::verify_is_predecessor(&db, blocks[1], blocks[2], Some(1)).unwrap());
        assert!(Validator::verify_is_predecessor(&db, blocks[1], blocks[1], None).unwrap());
    }
}
