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
    Address, Digest, ToDigest, ValidatorsVec,
    consensus::BatchCommitmentValidationReply,
    db::{AnnounceStorageRO, GlobalsStorageRO, OnChainStorageRO},
    ecdsa::{ContractSignature, PublicKey},
    events::{BlockRequestEvent, RouterRequestEvent, router::ProgramCreatedEvent},
    gear::{AggregatedPublicKey, BatchCommitment},
};
use gprimitives::{ActorId, H256, U256};
use gsigner::secp256k1::{Secp256k1SignerExt, Signer};
use parity_scale_codec::{Decode, Encode};
use rand::SeedableRng;
use roast_secp256k1_evm::frost::{
    Identifier,
    keys::{self, IdentifierList, VerifiableSecretSharingCommitment},
};
use std::collections::{BTreeMap, HashSet};

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
        let signature =
            signer.sign_for_contract_digest(router_address, pub_key, batch_digest, None)?;
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
    pub fn into_parts(self) -> (BatchCommitment, Vec<ContractSignature>) {
        (self.batch, self.signatures.into_values().collect())
    }
}
// TODO: #5019 this is a temporal solution. In future need to implement DKG algorithm.
pub fn generate_roast_keys(
    validators: &ValidatorsVec,
) -> Result<(AggregatedPublicKey, VerifiableSecretSharingCommitment)> {
    let validators_identifiers = validators
        .iter()
        .map(|validator| {
            let mut bytes = [0u8; 32];
            bytes[12..32].copy_from_slice(&validator.0);
            Identifier::deserialize(&bytes).unwrap()
        })
        .collect::<Vec<_>>();

    let identifiers = IdentifierList::Custom(&validators_identifiers);

    let rng = rand_chacha::ChaCha8Rng::from_seed([1u8; 32]);

    let (mut secret_shares, public_key_package) =
        keys::generate_with_dealer(validators.len() as u16, 1, identifiers, rng)?;

    let verifiable_secret_sharing_commitment = secret_shares
        .pop_first()
        .map(|(_key, value)| value.commitment().clone())
        .ok_or_else(|| anyhow!("Expect at least one identifier"))?;

    let public_key_compressed: [u8; 33] = public_key_package
        .verifying_key()
        .serialize()?
        .try_into()
        .map_err(|_| anyhow!("Failed to convert public key to compressed format"))?;
    let public_key_uncompressed = PublicKey::from_bytes(public_key_compressed)
        .expect("valid aggregated public key")
        .to_uncompressed();
    let (public_key_x_bytes, public_key_y_bytes) = public_key_uncompressed.split_at(32);

    let aggregated_public_key = AggregatedPublicKey {
        x: U256::from_big_endian(public_key_x_bytes),
        y: U256::from_big_endian(public_key_y_bytes),
    };

    Ok((aggregated_public_key, verifiable_secret_sharing_commitment))
}

pub fn has_duplicates<T: std::hash::Hash + Eq>(data: &[T]) -> bool {
    let mut seen = HashSet::new();
    data.iter().any(|item| !seen.insert(item))
}

pub fn block_touched_programs<DB: OnChainStorageRO + AnnounceStorageRO + GlobalsStorageRO>(
    db: &DB,
    block_hash: H256,
) -> Result<HashSet<ActorId>> {
    // NOTE: Using latest computed announce is not completely correct way to determine touched programs,
    // but it is good enough approximation, and it is enough for announce creation,
    // in worst case announce wouldn't be committed and it would become expired later.
    let mut known_programs = db
        .announce_program_states(db.globals().latest_computed_announce_hash)
        .ok_or_else(|| anyhow!("Not found program states for latest computed announce"))?
        .keys()
        .cloned()
        .collect::<HashSet<_>>();

    let touched_programs = db
        .block_events(block_hash)
        .ok_or_else(|| anyhow!("Events for block {block_hash} not found"))?
        .into_iter()
        .filter_map(|event| event.to_request())
        .filter_map(|request| match request {
            BlockRequestEvent::Router(RouterRequestEvent::ProgramCreated(
                ProgramCreatedEvent { actor_id, .. },
            )) => {
                known_programs.insert(actor_id);
                None
            }
            BlockRequestEvent::Mirror { actor_id, .. } if known_programs.contains(&actor_id) => {
                Some(actor_id)
            }
            _ => None,
        })
        .collect();

    Ok(touched_programs)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::*;
    use ethexe_common::mock::*;

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
    fn test_has_duplicates() {
        let data = vec![1, 2, 3, 4, 5];
        assert!(!has_duplicates(&data));

        let data = vec![1, 2, 3, 4, 5, 3];
        assert!(has_duplicates(&data));
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
                .sign_for_contract_digest(
                    ADDRESS,
                    other_pub_key,
                    multisigned_batch.batch_digest,
                    None,
                )
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
                .sign_for_contract_digest(ADDRESS, pub_key, incorrect_digest, None)
                .unwrap(),
        };

        let result = multisigned_batch.accept_batch_commitment_validation_reply(reply, |_| Ok(()));
        assert!(result.is_err());
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
                .sign_for_contract_digest(
                    ADDRESS,
                    other_pub_key,
                    multisigned_batch.batch_digest,
                    None,
                )
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
}
