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

use crate::{
    Announce, AnnounceHash, Digest, ToDigest,
    ecdsa::{ContractSignature, SignedData},
    gear::BatchCommitment,
};
use gprimitives::CodeId;
use k256::sha2::Digest as _;
use parity_scale_codec::{Decode, Encode};

pub type SignedAnnounce = SignedData<Announce>;
pub type SignedValidationRequest = SignedData<BatchCommitmentValidationRequest>;

/// Represents a request for validating a batch commitment.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct BatchCommitmentValidationRequest {
    // Digest of batch commitment to validate
    pub digest: Digest,
    /// Optional head announce hash of the chain commitment
    pub head: Option<AnnounceHash>,
    /// List of codes which are part of the batch
    pub codes: Vec<CodeId>,
    /// Whether validators commitment is part of the batch
    pub validators: bool,
    /// Whether rewards commitment is part of the batch
    pub rewards: bool,
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
            head: batch.chain_commitment.as_ref().map(|cc| cc.head_announce),
            codes,
            rewards: batch.rewards_commitment.is_some(),
            validators: batch.validators_commitment.is_some(),
        }
    }
}

impl ToDigest for BatchCommitmentValidationRequest {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let Self {
            digest,
            head,
            codes,
            rewards,
            validators,
        } = self;

        hasher.update(digest);
        head.map(|h| hasher.update(h.0));
        hasher.update(
            codes
                .iter()
                .flat_map(|h| h.into_bytes())
                .collect::<Vec<u8>>(),
        );
        hasher.update([*rewards as u8]);
        hasher.update([*validators as u8]);
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
