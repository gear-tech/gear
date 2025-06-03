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

use crate::ToDigest;
use alloc::vec::Vec;
use gear_core::message::{ReplyDetails, StoredMessage};
use gprimitives::{ActorId, CodeId, MessageId, H256, U256};
use parity_scale_codec::{Decode, Encode};
use roast_secp256k1_evm::frost::keys::VerifiableSecretSharingCommitment;
use sha3::Digest as _;

// TODO: support query from router.
pub const COMPUTATION_THRESHOLD: u64 = 2_500_000_000;
pub const SIGNING_THRESHOLD_PERCENTAGE: u16 = 6666;
pub const WVARA_PER_SECOND: u128 = 10_000_000_000_000;
pub type Address = [u8; 20];

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

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct BlockCommitment {
    pub hash: H256,
    /// represented as u48 in router contract.
    pub timestamp: u64,
    pub previous_committed_block: H256,
    pub predecessor_block: H256,
    pub transitions: Vec<StateTransition>,
}

impl ToDigest for BlockCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            hash,
            timestamp,
            previous_committed_block,
            predecessor_block,
            transitions,
        } = self;

        hasher.update(hash.as_bytes());
        hasher.update(crate::u64_into_uint48_be_bytes_lossy(*timestamp).as_slice());
        hasher.update(previous_committed_block.as_bytes());
        hasher.update(predecessor_block.as_bytes());
        hasher.update(transitions.to_digest().as_ref());
    }
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct CodeCommitment {
    pub id: CodeId,
    /// represented as u48 in router contract.
    pub timestamp: u64,
    pub valid: bool,
}

impl ToDigest for CodeCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            id,
            timestamp,
            valid,
        } = self;

        hasher.update(id.into_bytes().as_slice());
        hasher.update(crate::u64_into_uint48_be_bytes_lossy(*timestamp).as_slice());
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

        hasher.update(amount.to_little_endian());
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
                hasher.update(amount.to_little_endian());
            });

        hasher.update(total_amount.to_little_endian());
        hasher.update(token);
    }
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct RewardsCommitment {
    pub operators: OperatorRewardsCommitment,
    pub stakers: StakerRewardsCommitment,
    /// represented as u48 in router contract
    pub timestamp: u64,
}

impl ToDigest for RewardsCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            operators,
            stakers,
            timestamp,
        } = self;

        hasher.update(operators.to_digest().as_ref());
        hasher.update(stakers.to_digest().as_ref());
        hasher.update(crate::u64_into_uint48_be_bytes_lossy(*timestamp));
    }
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct BatchCommitment {
    pub block_commitments: Vec<BlockCommitment>,
    pub code_commitments: Vec<CodeCommitment>,
    pub rewards_commitments: Vec<RewardsCommitment>,
}

impl ToDigest for BatchCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        // To avoid missing incorrect hashing while developing.
        let Self {
            code_commitments,
            block_commitments,
            rewards_commitments,
        } = self;

        hasher.update(block_commitments.to_digest());
        hasher.update(code_commitments.to_digest());
        hasher.update(rewards_commitments.to_digest());
    }
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct ValidatorsCommitment {
    pub aggregated_public_key: AggregatedPublicKey,
    pub verifiable_secret_sharing_commitment: VerifiableSecretSharingCommitment,
    pub validators: Vec<ActorId>,
    pub era_index: u64,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub enum CodeState {
    #[default]
    Unknown,
    ValidationRequested,
    Validated,
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

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
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

        let (reply_details_to, reply_details_code) = reply_details.unwrap_or_default().into_parts();

        hasher.update(id.as_ref());
        hasher.update(destination.to_address_lossy().as_bytes());
        hasher.update(payload.as_slice());
        hasher.update(value.to_be_bytes().as_slice());
        hasher.update(reply_details_to.as_ref());
        hasher.update(reply_details_code.to_bytes().as_slice());
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

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct StateTransition {
    pub actor_id: ActorId,
    pub new_state_hash: H256,
    pub exited: bool,
    pub inheritor: ActorId,
    pub value_to_receive: u128,
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
            value_claims,
            messages,
        } = self;

        hasher.update(actor_id.to_address_lossy().as_bytes());
        hasher.update(new_state_hash.as_bytes());
        hasher.update([*exited as u8]);
        hasher.update(inheritor.to_address_lossy().as_bytes());
        hasher.update(value_to_receive.to_be_bytes().as_slice());
        hasher.update(value_claims.to_digest().as_ref());
        hasher.update(messages.to_digest().as_ref());
    }
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct ValidationSettings {
    pub signing_threshold_percentage: u16,
    pub validators: Vec<ActorId>,
    // flatten mapping of validators ActorId => bool
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueClaim {
    pub message_id: MessageId,
    pub destination: ActorId,
    pub value: u128,
}

/// Note: `ValueClaim` is not `ToDigest`
impl ToDigest for [ValueClaim] {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        self.iter().for_each(
            |ValueClaim {
                 message_id,
                 destination,
                 value,
             }| {
                hasher.update(message_id.as_ref());
                hasher.update(destination.to_address_lossy().as_bytes());
                hasher.update(value.to_be_bytes().as_slice());
            },
        )
    }
}

#[derive(Clone, Copy, Debug, Encode, Decode, PartialEq, Eq, Default, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum Origin {
    #[default]
    Ethereum,
    OffChain,
}
