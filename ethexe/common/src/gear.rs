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

//! This is supposed to be an exact copy of Gear.sol library.

use crate::{Address, Announce, Digest, HashOf, ToDigest, ValidatorsVec};
use alloc::vec::Vec;
use alloy_primitives::U256 as AlloyU256;
use gear_core::message::{ReplyCode, ReplyDetails, StoredMessage, SuccessReplyReason};
use gprimitives::{ActorId, CodeId, H256, MessageId, U256};
use parity_scale_codec::{Decode, Encode};
use roast_secp256k1_evm::frost::keys::VerifiableSecretSharingCommitment;
use sha3::Digest as _;

// TODO: support query from router.
pub const COMPUTATION_THRESHOLD: u64 = 2_500_000_000;
pub const WVARA_PER_SECOND: u128 = 10_000_000_000_000;

/// Gas limit for chunk processing.
pub const CHUNK_PROCESSING_GAS_LIMIT: u64 = 1_000_000_000_000;
/// Gas charge threshold for panicked injected messages.
pub const INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD: u64 = 1_000_000_000;

/// Max block gas limit for the node.
pub const MAX_BLOCK_GAS_LIMIT: u64 = 9_000_000_000_000;

/// [`CANONICAL_QUARANTINE`] defines the period of blocks to wait before applying canonical events.
pub const CANONICAL_QUARANTINE: u8 = 16;

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct AggregatedPublicKey {
    pub x: U256,
    pub y: U256,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
#[repr(u8)]
pub enum SignatureType {
    FROST,
    ECDSA,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct AddressBook {
    pub mirror: ActorId,
    pub mirror_proxy: ActorId,
    pub wrapped_vara: ActorId,
}

/// Squashed chain commitment that contains all state transitions and gear blocks.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct ChainCommitment {
    pub transitions: Vec<StateTransition>,
    pub head_announce: HashOf<Announce>,
}

impl ToDigest for ChainCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let ChainCommitment {
            transitions,
            head_announce,
        } = self;

        hasher.update(transitions.to_digest());
        hasher.update(head_announce.inner().0);
    }
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct CodeCommitment {
    pub id: CodeId,
    pub valid: bool,
}

impl ToDigest for CodeCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self { id, valid } = self;

        hasher.update(id.into_bytes());
        hasher.update([*valid as u8]);
    }
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct OperatorRewardsCommitment {
    pub amount: U256,
    pub root: H256,
}

impl ToDigest for OperatorRewardsCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let OperatorRewardsCommitment { amount, root } = self;

        hasher.update(<[u8; 32]>::from(*amount));
        hasher.update(root);
    }
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct StakerRewards {
    pub vault: Address,
    pub amount: U256,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct StakerRewardsCommitment {
    pub distribution: Vec<StakerRewards>,
    pub total_amount: U256,
    pub token: Address,
}

impl ToDigest for StakerRewardsCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let StakerRewardsCommitment {
            distribution,
            total_amount,
            token,
        } = &self;

        distribution
            .iter()
            .for_each(|StakerRewards { vault, amount }| {
                hasher.update(vault);
                hasher.update(<[u8; 32]>::from(*amount));
            });

        hasher.update(<[u8; 32]>::from(*total_amount));
        hasher.update(token);
    }
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct RewardsCommitment {
    pub operators: OperatorRewardsCommitment,
    pub stakers: StakerRewardsCommitment,
    /// Rewards for timestamp. Represented as u48 in router contract.
    pub timestamp: u64,
}

impl ToDigest for RewardsCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let RewardsCommitment {
            operators,
            stakers,
            timestamp,
        } = self;

        hasher.update(operators.to_digest());
        hasher.update(stakers.to_digest());
        hasher.update(crate::u64_into_uint48_be_bytes_lossy(*timestamp));
    }
}

/// Batch of different commitments that are created for a specific ethereum block.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct BatchCommitment {
    // Hash of ethereum block for which this batch has been created
    // This is used to identify whether router have to apply this batch,
    // it can be a batch from another branch and after reorg it's not actual anymore (currently we have predecessorBlock for this)
    pub block_hash: H256,

    /// Timestamp of ethereum block for which this batch has been created
    /// This timestamp is used to identify validator set to verify commitment (current or previous era)
    pub timestamp: u64,

    /// Digest of the previous committed batch.
    /// This is used to verify that the batch is committed in the correct order.
    pub previous_batch: Digest,

    /// How long the batch is valid (in blocks since `block_hash`).
    /// if 1 - then valid only in child block
    /// if 2 - then valid in child and grandchild blocks
    /// ... etc.
    pub expiry: u8,

    pub chain_commitment: Option<ChainCommitment>,
    pub code_commitments: Vec<CodeCommitment>,
    pub validators_commitment: Option<ValidatorsCommitment>,
    pub rewards_commitment: Option<RewardsCommitment>,
}

impl ToDigest for BatchCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            block_hash,
            timestamp,
            previous_batch,
            expiry,
            chain_commitment,
            code_commitments,
            validators_commitment,
            rewards_commitment,
        } = self;

        hasher.update(block_hash);
        hasher.update(crate::u64_into_uint48_be_bytes_lossy(*timestamp));
        hasher.update(previous_batch);
        hasher.update(expiry.to_be_bytes());
        hasher.update(chain_commitment.to_digest());
        hasher.update(code_commitments.to_digest());
        hasher.update(rewards_commitment.to_digest());
        hasher.update(validators_commitment.to_digest());
    }
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct Timelines {
    pub era: u64,
    pub election: u64,
    pub validation_delay: u64,
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct ValidatorsCommitment {
    pub aggregated_public_key: AggregatedPublicKey,
    pub verifiable_secret_sharing_commitment: VerifiableSecretSharingCommitment,
    pub validators: ValidatorsVec,
    pub era_index: u64,
}

impl ToDigest for ValidatorsCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let ValidatorsCommitment {
            aggregated_public_key,
            verifiable_secret_sharing_commitment: _, // TODO: add to digest
            validators,
            era_index,
        } = self;

        hasher.update(<[u8; 32]>::from(aggregated_public_key.x));
        hasher.update(<[u8; 32]>::from(aggregated_public_key.y));
        hasher.update(
            validators
                .iter()
                .flat_map(|v| {
                    // Adjust to 32 bytes, because of `encodePacked` in Gear.validatorCommitmentHash
                    let mut bytes = [0u8; 32];
                    bytes[12..32].copy_from_slice(&v.0);
                    bytes.into_iter()
                })
                .collect::<Vec<u8>>(),
        );

        let bytes = AlloyU256::from(*era_index).to_be_bytes::<32>();
        hasher.update(bytes);
    }
}

#[derive(Clone, Copy, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub enum CodeState {
    #[default]
    Unknown,
    ValidationRequested,
    Validated,
}

impl From<u8> for CodeState {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Unknown,
            1 => Self::ValidationRequested,
            2 => Self::Validated,
            // NOTE: newly added variants should be updated accordingly
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct CommittedBlockInfo {
    pub hash: H256,
    /// represented as u48 in router contract.
    pub timestamp: u64,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct ComputationSettings {
    pub threshold: u64,
    pub wvara_per_second: u128,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Message {
    pub id: MessageId,
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
    pub reply_details: Option<ReplyDetails>,
    pub call: bool,
}

impl ToDigest for Message {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            id,
            destination,
            payload,
            value,
            reply_details,
            call,
        } = self;

        let (reply_details_to, reply_details_code) =
            reply_details.map(|d| d.into_parts()).unwrap_or((
                MessageId::default(),
                ReplyCode::Success(SuccessReplyReason::Auto),
            ));

        hasher.update(id);
        hasher.update(destination.to_address_lossy());
        hasher.update(payload);
        hasher.update(value.to_be_bytes());
        hasher.update(reply_details_to);
        hasher.update(reply_details_code.to_bytes());
        hasher.update([*call as u8]);
    }
}

impl Message {
    pub fn from_stored(value: StoredMessage, call: bool) -> Self {
        let (id, _source, destination, payload, value, details) = value.into_parts();
        Self {
            id,
            destination,
            payload: payload.into_vec(),
            value,
            reply_details: details.and_then(|v| v.to_reply_details()),
            call,
        }
    }
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct ProtocolData {
    // flatten mapping of codes CodeId => CodeState
    // flatten mapping of program to codes ActorId => CodeId
    pub programs_count: U256,
    pub validated_codes_count: U256,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct StateTransition {
    pub actor_id: ActorId,
    pub new_state_hash: H256,
    pub exited: bool,
    pub inheritor: ActorId,
    /// We represent `value_to_receive` as `u128` and `bool` because each non-zero byte costs 16 gas,
    /// and each zero byte costs 4 gas (see <https://evm.codes/about#gascosts>).
    ///
    /// Negative numbers will be stored like this:
    /// ```
    /// $ cast
    /// > -1 ether
    /// Type: int256
    /// Hex: 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffff21f494c589c0000
    /// ```
    ///
    /// This is optimization on EVM side to reduce gas costs for storing and processing values.
    pub value_to_receive: u128,
    pub value_to_receive_negative_sign: bool,
    pub value_claims: Vec<ValueClaim>,
    pub messages: Vec<Message>,
}

impl ToDigest for StateTransition {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            actor_id,
            new_state_hash,
            exited,
            inheritor,
            value_to_receive,
            value_to_receive_negative_sign,
            value_claims,
            messages,
        } = self;

        hasher.update(actor_id.to_address_lossy());
        hasher.update(new_state_hash);
        hasher.update([*exited as u8]);
        hasher.update(inheritor.to_address_lossy());
        hasher.update(value_to_receive.to_be_bytes());
        hasher.update([*value_to_receive_negative_sign as u8]);
        // Match router's hashing strategy: keccak256 of concatenated value-claim bytes.
        hasher.update({
            let mut hasher = sha3::Keccak256::new();
            value_claims
                .iter()
                .for_each(|claim| claim.update_hasher(&mut hasher));
            hasher.finalize()
        });
        hasher.update(messages.to_digest());
    }
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueClaim {
    pub message_id: MessageId,
    pub destination: ActorId,
    pub value: u128,
}

impl ToDigest for ValueClaim {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let ValueClaim {
            message_id,
            destination,
            value,
        } = self;

        hasher.update(message_id);
        hasher.update(destination.to_address_lossy());
        hasher.update(value.to_be_bytes());
    }
}

#[derive(Clone, Copy, Debug, Encode, Decode, PartialEq, Eq, Default, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum MessageType {
    #[default]
    Canonical,
    Injected,
}
