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
    Address, Announce, Digest, HashOf, ProtocolTimelines, ToDigest,
    ecdsa::{ContractSignature, VerifiedData},
    gear::BatchCommitment,
    validators::ValidatorsVec,
};
use alloc::vec::Vec;
use core::num::NonZeroUsize;
use gprimitives::CodeId;
use k256::sha2::Digest as _;
use parity_scale_codec::{Decode, Encode};
use sha3::Keccak256;

/// The maximum batch size limit - 120 KB.
pub const MAX_BATCH_SIZE_LIMIT: u64 = 120 * 1024;

/// The default batch size - 100 KB.
pub const DEFAULT_BATCH_SIZE_LIMIT: u64 = 100 * 1024;

/// Default threshold for producer to submit commitment despite of no transitions
pub const DEFAULT_CHAIN_DEEPNESS_THRESHOLD: u32 = 500;

pub type VerifiedAnnounce = VerifiedData<Announce>;
pub type VerifiedValidationRequest = VerifiedData<BatchCommitmentValidationRequest>;
pub type VerifiedValidationReply = VerifiedData<BatchCommitmentValidationReply>;

// TODO #4553: temporary implementation, should be improved
/// Returns block producer index for time slot. Next slot is the next validator in the list.
pub const fn block_producer_index_for_slot(validators_amount: NonZeroUsize, slot: u64) -> usize {
    (slot % validators_amount.get() as u64) as usize
}

impl ProtocolTimelines {
    /// Calculates the producer address for a given timestamp.
    ///
    /// # Arguments
    /// * `validators` - A non-empty vector of validator addresses.
    /// * `timestamp` - The timestamp for which to calculate the block producer.
    ///
    /// Returns `None` if timestamp is before genesis.
    pub fn block_producer_at(&self, validators: &ValidatorsVec, timestamp: u64) -> Option<Address> {
        let idx = self.block_producer_index_at(validators.len_nonzero(), timestamp)?;
        validators.get(idx).cloned()
    }

    /// Calculates the block producer index for a given timestamp.
    ///
    /// # Arguments
    /// * `validators_amount` - The number of validators in the protocol.
    /// * `timestamp` - The timestamp for which to calculate the block producer index.
    ///
    /// Returns `None` if timestamp is before genesis.
    pub fn block_producer_index_at(
        &self,
        validators_amount: NonZeroUsize,
        timestamp: u64,
    ) -> Option<usize> {
        let slot = self.slot_from_ts(timestamp)?;
        Some(block_producer_index_for_slot(validators_amount, slot))
    }
}

/// Represents a request for validating a batch commitment.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq, Hash)]
pub struct BatchCommitmentValidationRequest {
    // Digest of batch commitment to validate
    pub digest: Digest,
    /// Optional head announce hash of the chain commitment
    pub head: Option<HashOf<Announce>>,
    /// List of codes which are part of the batch
    pub codes: Vec<CodeId>,
    /// Whether rewards commitment is part of the batch
    pub rewards: bool,
    /// Whether validators commitment is part of the batch
    pub validators: bool,
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
        head.map(|h| hasher.update(h.inner().0));
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
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq, Hash)]
pub struct BatchCommitmentValidationReply {
    /// Digest of the [`BatchCommitment`] being validated
    pub digest: Digest,
    /// Signature confirming the validation by origin
    pub signature: ContractSignature,
}

impl ToDigest for BatchCommitmentValidationReply {
    fn update_hasher(&self, hasher: &mut Keccak256) {
        let Self { digest, signature } = self;
        hasher.update(digest.0);
        hasher.update(signature.into_pre_eip155_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::num::NonZeroU64;

    #[test]
    fn block_producer_index_calculates_correct_index() {
        let validators_amount = NonZeroUsize::new(5).unwrap();
        let slot = 7;

        let index = block_producer_index_for_slot(validators_amount, slot);

        assert_eq!(index, 2);
    }

    #[test]
    fn block_producer_for_calculates_correct_producer() {
        let validators: ValidatorsVec = vec![
            Address::from([1; 20]),
            Address::from([2; 20]),
            Address::from([3; 20]),
        ]
        .try_into()
        .unwrap();

        let producer = ProtocolTimelines {
            slot: NonZeroU64::new(1).unwrap(),
            genesis_ts: 0,
            era: NonZeroU64::new(1).unwrap(),
            election: 0,
        }
        .block_producer_at(&validators, 10);

        assert_eq!(producer, Some(Address::from([2; 20])));
    }

    #[test]
    fn block_producer_for_calculates_correct_producer_with_genesis_timestamp() {
        let validators: ValidatorsVec = vec![
            Address::from([1; 20]),
            Address::from([2; 20]),
            Address::from([3; 20]),
        ]
        .try_into()
        .unwrap();

        let producer = ProtocolTimelines {
            slot: NonZeroU64::new(2).unwrap(),
            genesis_ts: 6,
            era: NonZeroU64::new(1).unwrap(),
            election: 0,
        }
        .block_producer_at(&validators, 16);

        assert_eq!(producer, Some(Address::from([3; 20])));
    }

    #[test]
    fn block_producer_at_returns_none_before_genesis() {
        let validators: ValidatorsVec = vec![Address::from([1; 20])].try_into().unwrap();

        let producer = ProtocolTimelines {
            slot: NonZeroU64::new(1).unwrap(),
            genesis_ts: 100,
            era: NonZeroU64::new(1).unwrap(),
            election: 0,
        }
        .block_producer_at(&validators, 50);

        assert_eq!(producer, None);
    }
}
