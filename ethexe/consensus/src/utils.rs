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

use anyhow::{Result, anyhow};
use ethexe_common::{
    Address, Digest, ProducerBlock, SimpleBlockData, ToDigest,
    db::{BlockMetaStorageRead, CodesStorageRead, OnChainStorageRead},
    ecdsa::{ContractSignature, PublicKey, SignedData},
    gear::{BatchCommitment, ChainCommitment, CodeCommitment},
    sha3::{self, digest::Digest as _},
};
use ethexe_signer::Signer;
use gprimitives::{CodeId, H256};
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::{BTreeMap, HashSet},
    hash::Hash,
};

pub type SignedProducerBlock = SignedData<ProducerBlock>;
pub type SignedValidationRequest = SignedData<BatchCommitmentValidationRequest>;

/// Represents a request for validating a batch commitment.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct BatchCommitmentValidationRequest {
    // Digest of batch commitment to validate
    pub digest: Digest,
    /// List of blocks to validate
    pub head: Option<H256>,
    /// List of codes which are part of the batch
    pub codes: Vec<CodeId>,
}

impl BatchCommitmentValidationRequest {
    pub fn new(batch: &BatchCommitment) -> Self {
        let codes = batch
            .code_commitments
            .iter()
            .map(|commitment| commitment.id)
            .collect();

        BatchCommitmentValidationRequest {
            digest: batch.to_digest(),
            head: batch.chain_commitment.as_ref().map(|c| c.head),
            codes,
        }
    }
}

impl ToDigest for BatchCommitmentValidationRequest {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self {
            digest,
            head,
            codes,
        } = self;

        hasher.update(digest);
        head.map(|head| hasher.update(head));
        hasher.update(
            codes
                .iter()
                .flat_map(|h| h.into_bytes())
                .collect::<Vec<u8>>(),
        );
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

pub fn aggregate_code_commitments<DB: CodesStorageRead>(
    db: &DB,
    codes: impl IntoIterator<Item = CodeId>,
    fail_if_not_found: bool,
) -> Result<Vec<CodeCommitment>> {
    let mut commitments = Vec::new();

    for id in codes {
        match db.code_valid(id) {
            Some(valid) => commitments.push(CodeCommitment { id, valid }),
            None if fail_if_not_found => {
                return Err(anyhow::anyhow!("Code status not found in db: {id}"));
            }
            None => {}
        }
    }

    Ok(commitments)
}

pub fn aggregate_chain_commitment<DB: BlockMetaStorageRead + OnChainStorageRead>(
    db: &DB,
    from_block_hash: H256,
    fail_if_not_computed: bool,
    max_deepness: Option<u32>,
) -> Result<Option<(ChainCommitment, u32)>> {
    // TODO #4744: improve squashing - removing redundant state transitions

    let last_committed_head = db
        .block_meta(from_block_hash)
        .last_committed_head
        .ok_or_else(|| {
            anyhow!("Cannot get from db last committed head for block {from_block_hash}")
        })?;

    let mut block_hash = from_block_hash;
    let mut counter: u32 = 0;
    let mut transitions = vec![];
    while block_hash != last_committed_head {
        if max_deepness.map(|d| counter >= d).unwrap_or(false) {
            return Err(anyhow!(
                "Chain commitment is too deep: {block_hash} at depth {counter}"
            ));
        }

        counter += 1;

        if !db.block_meta(block_hash).computed {
            // This can happen when validator syncs from p2p network and skips some old blocks.
            if fail_if_not_computed {
                return Err(anyhow!("Block {block_hash} is not computed"));
            } else {
                return Ok(None);
            }
        }

        transitions.push(db.block_outcome(block_hash).ok_or_else(|| {
            anyhow!("Cannot get from db outcome for computed block {block_hash}")
        })?);

        block_hash = db
            .block_header(block_hash)
            .ok_or_else(|| anyhow!("Cannot get from db header for computed block {block_hash}"))?
            .parent_hash;
    }

    Ok(Some((
        ChainCommitment {
            transitions: transitions.into_iter().rev().flatten().collect(),
            head: from_block_hash,
        },
        counter,
    )))
}

pub fn create_batch_commitment<DB: BlockMetaStorageRead>(
    db: &DB,
    block: &SimpleBlockData,
    chain_commitment: Option<ChainCommitment>,
    code_commitments: Vec<CodeCommitment>,
) -> Result<Option<BatchCommitment>> {
    if chain_commitment.is_none() && code_commitments.is_empty() {
        return Ok(None);
    }

    let last_committed = db
        .block_meta(block.hash)
        .last_committed_batch
        .ok_or_else(|| {
            anyhow!(
                "Cannot get from db last committed block for block {}",
                block.hash
            )
        })?;

    Ok(Some(BatchCommitment {
        block_hash: block.hash,
        timestamp: block.header.timestamp,
        previous_batch: last_committed,
        chain_commitment,
        code_commitments,
        validators_commitment: None,
        rewards_commitment: None,
    }))
}

pub fn has_duplicates<T: Hash + Eq>(data: &[T]) -> bool {
    let mut seen = HashSet::new();
    data.iter().any(|item| !seen.insert(item))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::*;
    use ethexe_common::db::{BlockMetaStorageWrite, CodesStorageWrite};
    use ethexe_db::Database;

    const ADDRESS: Address = Address([42; 20]);

    #[test]
    fn multisigned_batch_commitment_creation() {
        let batch = BatchCommitment::mock(());

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
        let batch = BatchCommitment::mock(());

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
        let batch = BatchCommitment::mock(());

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
        let batch = BatchCommitment::mock(());

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
    fn test_aggregate_chain_commitment() {
        let db = Database::memory();
        let BatchCommitment { block_hash, .. } = prepared_mock_batch_commitment(&db);

        let (commitment, counter) = aggregate_chain_commitment(&db, block_hash, false, None)
            .unwrap()
            .unwrap();
        assert_eq!(commitment.head, block_hash);
        assert_eq!(commitment.transitions.len(), 4);
        assert_eq!(counter, 3);

        let (commitment, counter) = aggregate_chain_commitment(&db, block_hash, true, None)
            .unwrap()
            .unwrap();
        assert_eq!(commitment.head, block_hash);
        assert_eq!(commitment.transitions.len(), 4);
        assert_eq!(counter, 3);

        aggregate_chain_commitment(&db, block_hash, false, Some(2)).unwrap_err();
        aggregate_chain_commitment(&db, block_hash, true, Some(2)).unwrap_err();

        db.mutate_block_meta(block_hash, |meta| meta.computed = false);
        assert!(
            aggregate_chain_commitment(&db, block_hash, false, None)
                .unwrap()
                .is_none()
        );
        aggregate_chain_commitment(&db, block_hash, true, None).unwrap_err();
    }

    #[test]
    fn test_aggregate_code_commitments() {
        let db = Database::memory();
        let codes = vec![CodeId::from([1; 32]), CodeId::from([2; 32])];

        // Test with valid codes
        db.set_code_valid(codes[0], true);
        db.set_code_valid(codes[1], false);

        let commitments = aggregate_code_commitments(&db, codes.clone(), false).unwrap();
        assert_eq!(
            commitments,
            vec![
                CodeCommitment {
                    id: codes[0],
                    valid: true,
                },
                CodeCommitment {
                    id: codes[1],
                    valid: false,
                }
            ]
        );

        let commitments =
            aggregate_code_commitments(&db, vec![codes[0], CodeId::from([3; 32]), codes[1]], false)
                .unwrap();
        assert_eq!(
            commitments,
            vec![
                CodeCommitment {
                    id: codes[0],
                    valid: true,
                },
                CodeCommitment {
                    id: codes[1],
                    valid: false,
                }
            ]
        );

        aggregate_code_commitments(&db, vec![CodeId::from([3; 32])], true).unwrap_err();
    }

    #[test]
    fn test_has_duplicates() {
        let data = vec![1, 2, 3, 4, 5];
        assert!(!has_duplicates(&data));

        let data = vec![1, 2, 3, 4, 5, 3];
        assert!(has_duplicates(&data));
    }
}
