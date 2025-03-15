// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use anyhow::{anyhow, ensure, Result};
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage, OnChainStorage},
    gear::{BlockCommitment, CodeCommitment},
    RoastData,
};
use ethexe_db::Database;
use ethexe_sequencer::agro::{self, AggregatedCommitments};
use ethexe_signer::{
    sha3::{self, Digest as _},
    Address, Digest, PrivateKey, PublicKey, Signature, Signer, ToDigest,
};
use gprimitives::{ActorId, H256};
use parity_scale_codec::{Decode, Encode};
use roast_secp256k1_evm::{
    frost::{
        keys::{KeyPackage, SecretShare, SigningShare, VerifiableSecretSharingCommitment},
        Identifier, SigningPackage,
    },
    Signer as RoastSigner,
};
use std::{
    collections::BTreeSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct Validator {
    db: Database,
    pub_key: PublicKey,
    pub_key_session: PublicKey,
    block_time: Duration,
    signer: Signer,
    identifier: Identifier,
    roast_signer: RoastSigner,
    router_address: Address,
    last_request_commitments_validation: u128,
    last_batch_commitment_digest: Digest,
}

pub struct Config {
    pub pub_key: PublicKey,
    pub pub_key_session: PublicKey,
    pub block_time: Duration,
    pub commitment: VerifiableSecretSharingCommitment,
    pub router_address: Address,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct BlockCommitmentValidationRequest {
    pub block_hash: H256,
    pub block_timestamp: u64,
    pub previous_committed_block: H256,
    pub predecessor_block: H256,
    pub transitions_digest: Digest,
}

impl From<&BlockCommitment> for BlockCommitmentValidationRequest {
    fn from(commitment: &BlockCommitment) -> Self {
        // To avoid missing incorrect hashing while developing.
        let BlockCommitment {
            hash,
            timestamp,
            previous_committed_block,
            predecessor_block,
            transitions,
        } = commitment;

        Self {
            block_hash: *hash,
            block_timestamp: *timestamp,
            previous_committed_block: *previous_committed_block,
            predecessor_block: *predecessor_block,
            transitions_digest: transitions.to_digest(),
        }
    }
}

impl ToDigest for BlockCommitmentValidationRequest {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            block_hash,
            block_timestamp,
            previous_committed_block,
            predecessor_block,
            transitions_digest,
        } = self;

        hasher.update(block_hash.as_bytes());
        hasher.update(ethexe_common::u64_into_uint48_be_bytes_lossy(*block_timestamp).as_slice());
        hasher.update(previous_committed_block.as_bytes());
        hasher.update(predecessor_block.as_bytes());
        hasher.update(transitions_digest.as_ref());
    }
}

impl Validator {
    pub fn new(config: &Config, db: Database, signer: Signer) -> Self {
        let &Config {
            pub_key,
            pub_key_session,
            block_time,
            router_address,
            ref commitment,
        } = config;

        let identifier = Identifier::deserialize(&ActorId::from(pub_key.to_address()).into_bytes())
            .expect("invalid identifier");
        let PrivateKey(private_key) = signer
            .get_private_key(pub_key_session)
            .expect("session private key not found");
        let signing_share = SigningShare::deserialize(&private_key).expect("invalid signing share");
        let secret_share = SecretShare::new(identifier, signing_share, commitment.clone());
        let key_package = KeyPackage::try_from(secret_share).expect("invalid key package");

        let mut rng = rand::thread_rng();
        let roast_signer = RoastSigner::new(key_package, &mut rng);

        Self {
            db,
            pub_key,
            pub_key_session,
            block_time,
            signer,
            identifier,
            roast_signer,
            router_address,
            last_request_commitments_validation: 0,
            last_batch_commitment_digest: Digest::from([0; 32]),
        }
    }

    pub fn pub_key(&self) -> PublicKey {
        self.pub_key
    }

    pub fn address(&self) -> Address {
        self.pub_key.to_address()
    }

    // TODO (gsobol): make test for this method
    pub fn aggregate_commitments_for_block(
        &self,
        block: H256,
    ) -> Result<Option<AggregatedCommitments<BlockCommitment>>> {
        let commitments_queue = self
            .db
            .block_commitment_queue(block)
            .ok_or_else(|| anyhow!("Block {block} is not in storage"))?;

        if commitments_queue.is_empty() {
            return Ok(None);
        }

        let mut commitments = Vec::new();

        let predecessor_block = block;

        for block in commitments_queue {
            // If there are not computed blocks in the queue, then we should skip aggregation this time.
            // This can happen when validator syncs from p2p network and skips some old blocks.
            if !self.db.block_computed(block) {
                log::warn!(
                    "Block {block} is not computed by some reasons, so skip the aggregation"
                );
                return Ok(None);
            }

            let outcomes = self
                .db
                .block_outcome(block)
                .ok_or_else(|| anyhow!("Cannot get from db outcome for computed block {block}"))?;

            let previous_committed_block =
                self.db.previous_not_empty_block(block).ok_or_else(|| {
                    anyhow!(
                        "Cannot get from db previous committed block for computed block {block}"
                    )
                })?;

            let header = self
                .db
                .block_header(block)
                .ok_or_else(|| anyhow!("Cannot get from db header for computed block {block}"))?;

            commitments.push(BlockCommitment {
                hash: block,
                timestamp: header.timestamp,
                previous_committed_block,
                predecessor_block,
                transitions: outcomes,
            });
        }

        self.aggregate(commitments).map(Some)
    }

    pub fn pub_key_session(&self) -> PublicKey {
        self.pub_key_session
    }

    pub fn aggregate<C: ToDigest>(&self, commitments: Vec<C>) -> Result<AggregatedCommitments<C>> {
        AggregatedCommitments::aggregate_commitments(
            commitments,
            &self.signer,
            self.pub_key,
            self.router_address,
        )
    }

    pub fn validate_batch_commitment(
        &mut self,
        codes_requests: impl IntoIterator<Item = CodeCommitment>,
        blocks_requests: impl IntoIterator<Item = BlockCommitmentValidationRequest>,
    ) -> Result<(RoastData, Signature)> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("failed to get time")
            .as_millis();
        let validation_round = (self.block_time / 4).as_millis();

        if now < self.last_request_commitments_validation + validation_round {
            return Err(anyhow!("Too frequent requests"));
        }

        let mut code_commitment_digests = Vec::new();

        for code_request in codes_requests {
            log::debug!("Receive code commitment for validation: {code_request:?}");
            code_commitment_digests.push(code_request.to_digest());
            Self::validate_code_commitment(&self.db, code_request)?;
        }

        let code_commitments_digest: Digest = code_commitment_digests.iter().collect();

        let mut block_commitment_digests = Vec::new();

        for block_request in blocks_requests.into_iter() {
            log::debug!("Receive block commitment for validation: {block_request:?}");
            block_commitment_digests.push(block_request.to_digest());
            Self::validate_block_commitment(&self.db, block_request)?;
        }

        let block_commitments_digest: Digest = block_commitment_digests.iter().collect();

        let batch_commitment_digest: Digest = [code_commitments_digest, block_commitments_digest]
            .iter()
            .collect();
        let batch_commitment_digest =
            agro::to_router_digest(batch_commitment_digest, self.router_address);

        let roast_data = RoastData {
            signature_share: None,
            signing_commitments: self.roast_signer.signing_commitments(),
        };
        let roast_data_digest = roast_data.to_digest();

        let ret = agro::sign_roast_data_digest(
            roast_data_digest,
            &self.signer,
            self.pub_key,
            self.router_address,
        )
        .map(|signature| (roast_data, signature));

        self.last_request_commitments_validation = now;
        self.last_batch_commitment_digest = batch_commitment_digest;

        ret
    }

    pub fn handle_session_start(
        &mut self,
        signers: BTreeSet<Identifier>,
        signing_package: SigningPackage,
    ) -> Result<(RoastData, Signature)> {
        if !signers.contains(&self.identifier) {
            return Err(anyhow!("This node is not in the list of signers"));
        }

        // TODO: maybe check len of signers == min_signers
        // TODO: maybe check that signers are validators

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("failed to get time")
            .as_millis();
        let validation_round = (self.block_time / 4).as_millis();

        if now >= self.last_request_commitments_validation + validation_round {
            return Err(anyhow!("Validation round ended"));
        }

        if signing_package.message() != self.last_batch_commitment_digest.as_ref() {
            return Err(anyhow!("Incorrect batch commitment digest"));
        }

        let mut rng = rand::thread_rng();

        let roast_data = RoastData {
            signature_share: Some(self.roast_signer.receive(&signing_package, &mut rng)?),
            signing_commitments: self.roast_signer.signing_commitments(),
        };
        let roast_data_digest = roast_data.to_digest();

        agro::sign_roast_data_digest(
            roast_data_digest,
            &self.signer,
            self.pub_key,
            self.router_address,
        )
        .map(|signature| (roast_data, signature))
    }

    fn validate_code_commitment<DB1: OnChainStorage + CodesStorage>(
        db: &DB1,
        request: CodeCommitment,
    ) -> Result<()> {
        let CodeCommitment {
            id,
            timestamp,
            valid,
        } = request;

        let local_timestamp = db
            .code_blob_info(id)
            .ok_or_else(|| anyhow!("Code {id} blob info is not in storage"))?
            .timestamp;

        if local_timestamp != timestamp {
            return Err(anyhow!("Requested and local code timestamps mismatch"));
        }

        let local_valid = db
            .code_valid(id)
            .ok_or_else(|| anyhow!("Code {id} is not validated by this node"))?;

        if local_valid != valid {
            return Err(anyhow!(
                "Requested and local code validation results mismatch"
            ));
        }

        Ok(())
    }

    fn validate_block_commitment<DB1: BlockMetaStorage + OnChainStorage>(
        db: &DB1,
        request: BlockCommitmentValidationRequest,
    ) -> Result<()> {
        let BlockCommitmentValidationRequest {
            block_hash,
            block_timestamp,
            previous_committed_block: allowed_previous_committed_block,
            predecessor_block: allowed_predecessor_block,
            transitions_digest,
        } = request;

        if !db.block_computed(block_hash) {
            return Err(anyhow!(
                "Requested block {block_hash} is not processed by this node"
            ));
        }

        let header = db.block_header(block_hash).ok_or_else(|| {
            anyhow!("Requested block {block_hash} header wasn't found in storage")
        })?;

        ensure!(header.timestamp == block_timestamp, "Timestamps mismatch");

        if db
            .block_outcome(block_hash)
            .ok_or_else(|| anyhow!("Cannot get from db outcome for block {block_hash}"))?
            .iter()
            .collect::<Digest>()
            != transitions_digest
        {
            return Err(anyhow!("Requested and local transitions digest mismatch"));
        }

        if db.previous_not_empty_block(block_hash).ok_or_else(|| {
            anyhow!("Cannot get from db previous not empty for block {block_hash}")
        })? != allowed_previous_committed_block
        {
            return Err(anyhow!(
                "Requested and local previous commitment block hash mismatch"
            ));
        }

        if !Self::verify_is_predecessor(db, allowed_predecessor_block, block_hash, None)? {
            return Err(anyhow!(
                "{block_hash} is not a predecessor of {allowed_predecessor_block}"
            ));
        }

        Ok(())
    }

    /// Verify whether `pred_hash` is a predecessor of `block_hash` in the chain.
    fn verify_is_predecessor(
        db: &impl OnChainStorage,
        block_hash: H256,
        pred_hash: H256,
        max_distance: Option<u32>,
    ) -> Result<bool> {
        if block_hash == pred_hash {
            return Ok(true);
        }

        let block_header = db
            .block_header(block_hash)
            .ok_or_else(|| anyhow!("header not found for block: {block_hash}"))?;

        if block_header.parent_hash == pred_hash {
            return Ok(true);
        }

        let pred_height = db
            .block_header(pred_hash)
            .ok_or_else(|| anyhow!("header not found for pred block: {pred_hash}"))?
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
                .ok_or_else(|| anyhow!("header not found for block: {block_hash}"))?
                .parent_hash;
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::gear::StateTransition;
    use ethexe_db::{BlockHeader, CodeInfo, Database};
    use gprimitives::CodeId;

    #[test]
    fn block_validation_request_digest() {
        let transition = StateTransition {
            actor_id: H256::random().0.into(),
            new_state_hash: H256::random(),
            inheritor: H256::random().0.into(),
            value_to_receive: 123,
            value_claims: vec![],
            messages: vec![],
        };

        let commitment = BlockCommitment {
            hash: H256::random(),
            timestamp: rand::random(),
            previous_committed_block: H256::random(),
            predecessor_block: H256::random(),
            transitions: vec![transition.clone(), transition],
        };

        assert_eq!(
            commitment.to_digest(),
            BlockCommitmentValidationRequest::from(&commitment).to_digest()
        );
    }

    #[test]
    fn test_validate_code_commitments() {
        let db = Database::from_one(&ethexe_db::MemDb::default(), [0; 20]);

        let code_id = CodeId::from(H256::random());

        Validator::validate_code_commitment(
            &db,
            CodeCommitment {
                id: code_id,
                timestamp: 42,
                valid: true,
            },
        )
        .expect_err("Code is not in db");

        db.set_code_valid(code_id, true);
        db.set_code_blob_info(
            code_id,
            CodeInfo {
                timestamp: 42,
                tx_hash: H256::random(),
            },
        );

        Validator::validate_code_commitment(
            &db,
            CodeCommitment {
                id: code_id,
                timestamp: 42,
                valid: false,
            },
        )
        .expect_err("Code validation result mismatch");

        Validator::validate_code_commitment(
            &db,
            CodeCommitment {
                id: code_id,
                timestamp: 42,
                valid: true,
            },
        )
        .unwrap();
    }

    #[test]
    fn test_validate_block_commitment() {
        let db = Database::from_one(&ethexe_db::MemDb::default(), [0; 20]);

        let block_hash = H256::random();
        let block_timestamp = rand::random::<u32>() as u64;
        let pred_block_hash = H256::random();
        let previous_committed_block = H256::random();
        let transitions = vec![];
        let transitions_digest = transitions.to_digest();

        db.set_block_computed(block_hash);
        db.set_block_outcome(block_hash, transitions);
        db.set_previous_not_empty_block(block_hash, previous_committed_block);
        db.set_block_header(
            block_hash,
            BlockHeader {
                height: 100,
                timestamp: block_timestamp,
                parent_hash: pred_block_hash,
            },
        );

        Validator::validate_block_commitment(
            &db,
            BlockCommitmentValidationRequest {
                block_hash,
                block_timestamp,
                previous_committed_block,
                predecessor_block: block_hash,
                transitions_digest,
            },
        )
        .unwrap();

        Validator::validate_block_commitment(
            &db,
            BlockCommitmentValidationRequest {
                block_hash,
                block_timestamp: block_timestamp + 1,
                previous_committed_block,
                predecessor_block: block_hash,
                transitions_digest,
            },
        )
        .expect_err("Timestamps mismatch");

        Validator::validate_block_commitment(
            &db,
            BlockCommitmentValidationRequest {
                block_hash,
                block_timestamp,
                previous_committed_block,
                predecessor_block: H256::random(),
                transitions_digest,
            },
        )
        .expect_err("Unknown pred block is provided");

        Validator::validate_block_commitment(
            &db,
            BlockCommitmentValidationRequest {
                block_hash,
                block_timestamp,
                previous_committed_block: H256::random(),
                predecessor_block: block_hash,
                transitions_digest,
            },
        )
        .expect_err("Unknown prev commitment is provided");

        Validator::validate_block_commitment(
            &db,
            BlockCommitmentValidationRequest {
                block_hash,
                block_timestamp,
                previous_committed_block,
                predecessor_block: block_hash,
                transitions_digest: Digest::from([2; 32]),
            },
        )
        .expect_err("Transitions digest mismatch");

        Validator::validate_block_commitment(
            &db,
            BlockCommitmentValidationRequest {
                block_hash: H256::random(),
                block_timestamp,
                previous_committed_block,
                predecessor_block: block_hash,
                transitions_digest,
            },
        )
        .expect_err("Block is not processed by this node");
    }

    #[test]
    fn test_verify_is_predecessor() {
        let db = Database::from_one(&ethexe_db::MemDb::default(), [0; 20]);

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
