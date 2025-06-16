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

//! # Utilities Module
//!
//! This module provides utility functions and data structures for handling batch commitments,
//! validation requests, and multi-signature operations in the Ethexe system.

use anyhow::Result;
use ethexe_common::{
    ecdsa::{ContractSignature, PublicKey},
    gear::{BatchCommitment, BlockCommitment, CodeCommitment},
    sha3::{self, Digest as _},
    Address, Digest, ToDigest,
};
use ethexe_signer::Signer;
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use std::collections::BTreeMap;

/// Represents a request for validating a batch of block commitments.
/// This structure is used to verify the integrity and validity of multiple block commitments
/// and their associated code commitments.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct BatchCommitmentValidationRequest {
    /// List of block commitment validation requests
    pub blocks: Vec<BlockCommitmentValidationRequest>,
    /// List of code commitments to be validated
    pub codes: Vec<CodeCommitment>,
}

impl BatchCommitmentValidationRequest {
    pub fn new(batch: &BatchCommitment) -> Self {
        BatchCommitmentValidationRequest {
            blocks: batch
                .block_commitments
                .iter()
                .map(BlockCommitmentValidationRequest::new)
                .collect(),
            codes: batch.code_commitments.clone(),
        }
    }
}

impl ToDigest for BatchCommitmentValidationRequest {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        hasher.update(self.blocks.to_digest());
        hasher.update(self.codes.to_digest());
        hasher.update([0u8; 0].to_digest());
    }
}

/// A request for validating a single block commitment.
/// Contains all necessary information to verify the integrity of a block's commitment.
///
/// NOTE: [`BlockCommitmentValidationRequest`] digest is always equal to the corresponding
/// [`BlockCommitment`] digest.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct BlockCommitmentValidationRequest {
    /// Hash of the block being validated
    pub block_hash: H256,
    /// Timestamp of the block
    pub block_timestamp: u64,
    /// Hash of the previous non-empty block
    pub previous_non_empty_block: H256,
    /// Hash of the predecessor block
    pub predecessor_block: H256,
    /// Digest of the block's state transitions
    pub transitions_digest: Digest,
}

impl BlockCommitmentValidationRequest {
    pub fn new(commitment: &BlockCommitment) -> Self {
        BlockCommitmentValidationRequest {
            block_hash: commitment.hash,
            block_timestamp: commitment.timestamp,
            previous_non_empty_block: commitment.previous_committed_block,
            predecessor_block: commitment.predecessor_block,
            transitions_digest: commitment.transitions.to_digest(),
        }
    }
}

impl ToDigest for BlockCommitmentValidationRequest {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self {
            block_hash,
            block_timestamp,
            previous_non_empty_block,
            predecessor_block,
            transitions_digest,
        } = self;

        hasher.update(block_hash);
        hasher.update(ethexe_common::u64_into_uint48_be_bytes_lossy(
            *block_timestamp,
        ));
        hasher.update(previous_non_empty_block);
        hasher.update(predecessor_block);
        hasher.update(transitions_digest);
    }
}

/// A reply to a batch commitment validation request.
/// Contains the digest of the batch and a signature confirming the validation.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct BatchCommitmentValidationReply {
    /// Digest of the [`BatchCommitment`] being validated
    pub digest: Digest,
    /// Signature confirming the validation by origin
    pub signature: ContractSignature,
}

/// A batch commitment, that has been signed by multiple validators.
/// This structure manages the collection of signatures from different validators
/// for a single batch commitment.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct MultisignedBatchCommitment {
    batch: BatchCommitment,
    batch_digest: Digest,
    router_address: Address,
    signatures: BTreeMap<Address, ContractSignature>,
}

impl MultisignedBatchCommitment {
    /// Creates a new multisigned batch commitment with an initial signature.
    ///
    /// # Arguments
    /// * `batch` - The batch commitment to be signed
    /// * `signer` - The contract signer used to create signatures
    /// * `pub_key` - The public key of the initial signer
    ///
    /// # Returns
    /// A new `MultisignedBatchCommitment` instance with the initial signature
    pub fn new(
        batch: BatchCommitment,
        signer: &Signer,
        router_address: Address,
        pub_key: PublicKey,
    ) -> Result<Self> {
        let batch_digest = batch.to_digest();
        let signature = signer.sign_for_contract(router_address, pub_key, batch_digest)?;
        let signatures: BTreeMap<_, _> = [(pub_key.to_address(), signature)].into_iter().collect();

        Ok(Self {
            batch,
            batch_digest,
            router_address,
            signatures,
        })
    }

    /// Accepts a validation reply from another validator and adds it's signature.
    ///
    /// # Arguments
    /// * `reply` - The validation reply containing the signature
    /// * `check_origin` - A closure to verify the origin of the signature
    ///
    /// # Returns
    /// Result indicating success or failure of the operation
    pub fn accept_batch_commitment_validation_reply(
        &mut self,
        reply: BatchCommitmentValidationReply,
        check_origin: impl FnOnce(Address) -> Result<()>,
    ) -> Result<()> {
        let BatchCommitmentValidationReply { digest, signature } = reply;

        anyhow::ensure!(digest == self.batch_digest, "Invalid reply digest");

        let origin = signature
            .validate(self.router_address, digest)?
            .to_address();

        check_origin(origin)?;

        self.signatures.insert(origin, signature);

        Ok(())
    }

    /// Returns a reference to the map of validator addresses to their signatures
    pub fn signatures(&self) -> &BTreeMap<Address, ContractSignature> {
        &self.signatures
    }

    /// Returns a reference to the underlying batch commitment
    pub fn batch(&self) -> &BatchCommitment {
        &self.batch
    }

    /// Consumes the structure and returns its parts
    ///
    /// # Returns
    /// A tuple containing the batch commitment and the map of signatures
    pub fn into_parts(self) -> (BatchCommitment, BTreeMap<Address, ContractSignature>) {
        (self.batch, self.signatures)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::*;

    const ADDRESS: Address = Address([42; 20]);

    #[test]
    fn multisigned_batch_commitment_creation() {
        let batch = BatchCommitment {
            block_commitments: vec![],
            code_commitments: vec![],
            rewards_commitments: vec![],
        };

        let (signer, _, public_keys) = init_signer_with_keys(1);
        let pub_key = public_keys[0];

        let multisigned_batch =
            MultisignedBatchCommitment::new(batch.clone(), &signer, ADDRESS, pub_key)
                .expect("Failed to create multisigned batch commitment");

        assert_eq!(multisigned_batch.batch, batch);
        assert_eq!(multisigned_batch.signatures.len(), 1);
    }

    #[test]
    fn accept_batch_commitment_validation_reply() {
        let batch = BatchCommitment {
            block_commitments: vec![],
            code_commitments: vec![],
            rewards_commitments: vec![],
        };

        let (signer, _, public_keys) = init_signer_with_keys(2);
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, ADDRESS, pub_key).unwrap();

        let other_pub_key = public_keys[1];
        let reply = BatchCommitmentValidationReply {
            digest: multisigned_batch.batch_digest,
            signature: signer
                .sign_for_contract(ADDRESS, other_pub_key, multisigned_batch.batch_digest)
                .unwrap(),
        };

        multisigned_batch
            .accept_batch_commitment_validation_reply(reply.clone(), |_| Ok(()))
            .expect("Failed to accept batch commitment validation reply");

        assert_eq!(multisigned_batch.signatures.len(), 2);

        // Attempt to add the same reply again
        multisigned_batch
            .accept_batch_commitment_validation_reply(reply, |_| Ok(()))
            .expect("Failed to accept batch commitment validation reply");

        // Ensure the number of signatures has not increased
        assert_eq!(multisigned_batch.signatures.len(), 2);
    }

    #[test]
    fn reject_validation_reply_with_incorrect_digest() {
        let batch = BatchCommitment {
            block_commitments: vec![],
            code_commitments: vec![],
            rewards_commitments: vec![],
        };

        let (signer, _, public_keys) = init_signer_with_keys(1);
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, ADDRESS, pub_key).unwrap();

        let incorrect_digest = [1, 2, 3].to_digest();
        let reply = BatchCommitmentValidationReply {
            digest: incorrect_digest,
            signature: signer
                .sign_for_contract(ADDRESS, pub_key, incorrect_digest)
                .unwrap(),
        };

        let result = multisigned_batch.accept_batch_commitment_validation_reply(reply, |_| Ok(()));
        assert!(result.is_err());
        assert_eq!(multisigned_batch.signatures.len(), 1);
    }

    #[test]
    fn check_origin_closure_behavior() {
        let batch = BatchCommitment {
            block_commitments: vec![],
            code_commitments: vec![],
            rewards_commitments: vec![],
        };

        let (signer, _, public_keys) = init_signer_with_keys(2);
        let pub_key = public_keys[0];

        let mut multisigned_batch =
            MultisignedBatchCommitment::new(batch, &signer, ADDRESS, pub_key).unwrap();

        let other_pub_key = public_keys[1];
        let reply = BatchCommitmentValidationReply {
            digest: multisigned_batch.batch_digest,
            signature: signer
                .sign_for_contract(ADDRESS, other_pub_key, multisigned_batch.batch_digest)
                .unwrap(),
        };

        // Case 1: check_origin allows the origin
        let result =
            multisigned_batch.accept_batch_commitment_validation_reply(reply.clone(), |_| Ok(()));
        assert!(result.is_ok());
        assert_eq!(multisigned_batch.signatures.len(), 2);

        // Case 2: check_origin rejects the origin
        let result = multisigned_batch.accept_batch_commitment_validation_reply(reply, |_| {
            anyhow::bail!("Origin not allowed")
        });
        assert!(result.is_err());
        assert_eq!(multisigned_batch.signatures.len(), 2);
    }

    #[test]
    fn signature_compatibility_between_validation_request_and_batch_commitment() {
        let batch = BatchCommitment {
            block_commitments: vec![],
            code_commitments: vec![CodeCommitment {
                id: H256::random().into(),
                timestamp: 123,
                valid: false,
            }],
            rewards_commitments: vec![],
        };
        let batch_validation_request = BatchCommitmentValidationRequest::new(&batch);
        assert_eq!(batch.to_digest(), batch_validation_request.to_digest());

        let (signer, _, public_keys) = init_signer_with_keys(1);
        let public_key = public_keys[0];

        let batch_signature = signer
            .sign_for_contract(ADDRESS, public_key, &batch)
            .unwrap();
        let validation_request_signature = signer
            .sign_for_contract(ADDRESS, public_key, &batch_validation_request)
            .unwrap();
        assert_eq!(batch_signature, validation_request_signature);

        let pk1 = batch_signature
            .validate(ADDRESS, batch.to_digest())
            .unwrap();
        let pk2 = validation_request_signature
            .validate(ADDRESS, batch_validation_request.to_digest())
            .unwrap();
        assert_eq!(pk1, pk2);
    }
}
