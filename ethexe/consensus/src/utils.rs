// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Utilities Module
//!
//! This module provides utility functions and data structures for handling batch commitments,
//! validation requests, and multi-signature operations in the Ethexe system.

use anyhow::{Result, anyhow};
use ethexe_common::{
    Address, Digest, ToDigest,
    consensus::BatchCommitmentValidationReply,
    db::OnChainStorageRO,
    ecdsa::{ContractSignature, PublicKey},
    gear::BatchCommitment,
};
use gprimitives::H256;
use gsigner::secp256k1::{Secp256k1SignerExt, Signer};
use parity_scale_codec::{Decode, Encode};
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
pub fn has_duplicates<T: std::hash::Hash + Eq>(data: &[T]) -> bool {
    let mut seen = HashSet::new();
    data.iter().any(|item| !seen.insert(item))
}

/// `target` lies on the canonical eth chain ending at `head` — i.e., `head`
/// is `target` itself or one of its descendants reachable via parent links.
/// `target == H256::zero()` is the genesis sentinel and returns `Ok(true)`.
pub fn is_eth_block_canonical_to<DB: OnChainStorageRO>(
    db: &DB,
    target: H256,
    head: H256,
) -> Result<bool> {
    if target.is_zero() {
        return Ok(true);
    }
    let target_height = db
        .block_header(target)
        .ok_or_else(|| anyhow!("eth chain walk: missing header for target {target}"))?
        .height;

    let mut current = head;
    loop {
        if current == target {
            return Ok(true);
        }
        if current.is_zero() {
            return Ok(false);
        }
        let header = db
            .block_header(current)
            .ok_or_else(|| anyhow!("eth chain walk: missing header for {current}"))?;
        if header.height <= target_height {
            return Ok(false);
        }
        current = header.parent_hash;
    }
}
