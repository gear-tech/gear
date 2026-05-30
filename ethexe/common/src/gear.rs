// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! This is supposed to be an exact copy of Gear.sol library.

use crate::{Address, Digest, ToDigest, ValidatorsVec};
use alloc::vec::Vec;
use alloy_primitives::U256 as AlloyU256;
use gear_core::message::{ReplyCode, ReplyDetails, StoredMessage, SuccessReplyReason};
use gprimitives::{ActorId, CodeId, H256, MessageId, U256};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sha3::Digest as _;

// TODO: support query from router.
/// Default gas threshold for a single computation unit, measured in gas units.
pub const COMPUTATION_THRESHOLD: u64 = 2_500_000_000;
/// Token reward rate expressed as wVARA (the smallest unit) emitted per second of computation.
pub const WVARA_PER_SECOND: u128 = 10_000_000_000_000;

/// Gas limit for chunk processing.
pub const CHUNK_PROCESSING_GAS_LIMIT: u64 = 1_000_000_000_000;
/// Gas charge threshold for panicked injected messages.
pub const INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD: u64 = 1_000_000_000;

/// Max block gas limit for the node.
pub const MAX_BLOCK_GAS_LIMIT: u64 = 9_000_000_000_000;

/// [`CANONICAL_QUARANTINE`] defines the period of blocks to wait before applying canonical events.
pub const CANONICAL_QUARANTINE: u8 = 16;

/// Aggregated FROST public key represented as an affine point on the elliptic curve.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct AggregatedPublicKey {
    /// X coordinate of the public key point.
    pub x: U256,
    /// Y coordinate of the public key point.
    pub y: U256,
}

/// Discriminator for the signature scheme used in a validator commitment.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
#[repr(u8)]
pub enum SignatureType {
    /// Flexible Round-Optimized Schnorr Threshold (FROST) multi-party signature.
    FROST,
    /// Standard Ethereum ECDSA signature.
    ECDSA,
}

/// On-chain addresses of the core ethexe contracts used by the router.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct AddressBook {
    /// Address of the Mirror contract implementation.
    pub mirror: ActorId,
    /// Address of the Mirror proxy (cloneable beacon).
    pub mirror_proxy: ActorId,
    /// Address of the WrappedVara ERC-20 contract.
    pub wrapped_vara: ActorId,
}

/// Squashed chain commitment with state transitions, MB head, and the latest
/// advanced Ethereum block hash, zero if no ethereum block has been advanced.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct ChainCommitment {
    /// Ordered list of program state transitions included in this commitment.
    pub transitions: Vec<StateTransition>,
    /// Hash of the Gear chain head block covered by this commitment.
    pub head: H256,
    /// Hash of the most recent Ethereum block that has been advanced; zero if none.
    pub last_advanced_eth_block: H256,
}

impl ToDigest for ChainCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let ChainCommitment {
            transitions,
            head,
            last_advanced_eth_block,
        } = self;

        hasher.update(transitions.to_digest());
        hasher.update(head.0);
        hasher.update(last_advanced_eth_block.0);
    }
}

/// Validator commitment recording whether a given code blob passed validation.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct CodeCommitment {
    /// Identifier of the code blob being committed.
    pub id: CodeId,
    /// `true` if the code passed validation; `false` if it was rejected.
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

/// Commitment to the total operator rewards for a batch, with a Merkle root for individual claims.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct OperatorRewardsCommitment {
    /// Total reward amount distributed to operators.
    pub amount: U256,
    /// Merkle root of the per-operator reward tree used to verify individual claims.
    pub root: H256,
}

impl ToDigest for OperatorRewardsCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let OperatorRewardsCommitment { amount, root } = self;

        hasher.update(<[u8; 32]>::from(*amount));
        hasher.update(root);
    }
}

/// Reward allocation for a single staker vault.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct StakerRewards {
    /// Ethereum address of the vault receiving the rewards.
    pub vault: Address,
    /// Amount of rewards allocated to this vault.
    pub amount: U256,
}

/// Commitment to the full staker reward distribution for a batch.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct StakerRewardsCommitment {
    /// Per-vault reward allocations that sum to `total_amount`.
    pub distribution: Vec<StakerRewards>,
    /// Total reward amount across all vaults.
    pub total_amount: U256,
    /// Address of the ERC-20 token in which rewards are denominated.
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

/// Combined reward commitment covering both operator and staker reward distributions for a batch.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct RewardsCommitment {
    /// Reward commitment for node operators.
    pub operators: OperatorRewardsCommitment,
    /// Reward commitment for stakers across vaults.
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

    /// Optional commitment to chain state transitions; absent when no programs were executed.
    pub chain_commitment: Option<ChainCommitment>,
    /// Commitments to code blob validation results included in this batch.
    pub code_commitments: Vec<CodeCommitment>,
    /// Optional commitment to a validator set change; absent when the set is unchanged.
    pub validators_commitment: Option<ValidatorsCommitment>,
    /// Optional commitment to reward distributions; absent when no rewards are issued.
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

/// Block-count durations governing era rotation, validator election, and validation scheduling.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct Timelines {
    /// Number of Ethereum blocks in one era.
    pub era: u64,
    /// Number of blocks before era end at which the election is held.
    pub election: u64,
    /// Number of blocks to wait after a chain head before requesting validation.
    pub validation_delay: u64,
}

/// Commitment to a new validator set, including the FROST key material and era index.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq)]
pub struct ValidatorsCommitment {
    /// Does the batch have aggregated public key in validators commitment.
    pub has_aggregated_public_key: bool,
    /// Combined FROST public key for the new validator set; meaningful only when `has_aggregated_public_key` is `true`.
    pub aggregated_public_key: AggregatedPublicKey,
    /// Raw bytes of the verifiable secret-sharing commitment for the DKG round.
    pub verifiable_secret_sharing_commitment: Vec<u8>,
    /// Ordered list of Ethereum addresses of the new validators.
    pub validators: ValidatorsVec,
    /// Index of the era that this validator set will govern.
    pub era_index: u64,
}

impl ToDigest for ValidatorsCommitment {
    fn update_hasher(&self, hasher: &mut sha3::Keccak256) {
        let ValidatorsCommitment {
            has_aggregated_public_key,
            aggregated_public_key,
            verifiable_secret_sharing_commitment: _, // TODO: add to digest
            validators,
            era_index,
        } = self;

        hasher.update([*has_aggregated_public_key as u8]);
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

/// Tracks the validation lifecycle of a code blob on-chain.
#[derive(Clone, Copy, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub enum CodeState {
    /// The code has not been seen or its state is unrecognized.
    #[default]
    Unknown,
    /// Validation of the code has been requested but not yet committed.
    ValidationRequested,
    /// The code has been validated and accepted by the validator set.
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

/// Identifies the last Ethereum block that was fully committed by the router.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct CommittedBlockInfo {
    /// Hash of the committed Ethereum block.
    pub hash: H256,
    /// represented as u48 in router contract.
    pub timestamp: u64,
}

/// On-chain parameters controlling how computation costs are priced and billed.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct ComputationSettings {
    /// Gas threshold per computation unit; mirrors `COMPUTATION_THRESHOLD` when read from the router.
    pub threshold: u64,
    /// wVARA token units charged per second of computation; mirrors `WVARA_PER_SECOND`.
    pub wvara_per_second: u128,
}

/// A Gear message included in a [`StateTransition`], mirroring the on-chain `Gear.Message` struct.
#[derive(Clone, Debug, Default, Encode, Decode, TypeInfo, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct Message {
    /// Unique identifier of the message.
    pub id: MessageId,
    /// Program or user account that should receive the message.
    pub destination: ActorId,
    /// Encoded message payload bytes.
    pub payload: Vec<u8>,
    /// Value (in the smallest token unit) attached to the message.
    pub value: u128,
    /// Present when the message is a reply; carries the origin message id and reply code.
    pub reply_details: Option<ReplyDetails>,
    /// When `true`, the message is treated as a synchronous call rather than an async send.
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
    /// Converts a `StoredMessage` into a [`Message`], discarding the source and setting `call`.
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

/// Aggregate protocol counters read from the router contract.
#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct ProtocolData {
    // flatten mapping of codes CodeId => CodeState
    // flatten mapping of program to codes ActorId => CodeId
    /// Total number of programs registered in the router.
    pub programs_count: U256,
    /// Number of code blobs that have been successfully validated.
    pub validated_codes_count: U256,
}

#[derive(Clone, Debug, Default, Encode, Decode, TypeInfo, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct StateTransition {
    /// Identifier of the program whose state changed.
    pub actor_id: ActorId,
    /// Hash of the program's new state after execution.
    pub new_state_hash: H256,
    /// `true` when the program called `gr_exit` and is no longer active.
    pub exited: bool,
    /// Program that inherits the exited program's balance; zero address if not applicable.
    pub inheritor: ActorId,
    /// We represent `value_to_receive` as `u128` and `bool` because each non-zero byte costs 16 gas,
    /// and each zero byte costs 4 gas (see <https://evm.codes/about#gascosts>).
    ///
    /// Negative numbers will be stored like this:
    /// ```bash
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
        hasher.update(value_claims.to_digest());
        hasher.update(messages.to_digest());
    }
}

/// A request to transfer value from an expired or claimed mailbox message to a destination.
#[derive(Clone, Debug, Default, Encode, Decode, TypeInfo, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueClaim {
    /// Identifier of the mailbox message whose value is being claimed.
    pub message_id: MessageId,
    /// Address that should receive the claimed value.
    pub destination: ActorId,
    /// Amount of value (in smallest token unit) to transfer.
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

/// Distinguishes messages that originate from the canonical Gear chain from those injected externally.
#[derive(
    Clone,
    Copy,
    Debug,
    Encode,
    Decode,
    PartialEq,
    Eq,
    Default,
    PartialOrd,
    Ord,
    Hash,
    derive_more::IsVariant,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum MessageType {
    /// Message produced by normal Gear chain execution.
    #[default]
    Canonical,
    /// Message injected from an external source (e.g., an Ethereum cross-chain call).
    Injected,
}

/// Identifying information about the Ethereum genesis block used to anchor the ethexe deployment.
#[derive(Debug)]
pub struct GenesisBlockInfo {
    /// Hash of the genesis Ethereum block.
    pub hash: H256,
    /// Block number of the genesis Ethereum block.
    pub number: u32,
    /// Unix timestamp of the genesis Ethereum block, in seconds.
    pub timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validators_commitment_accepts_raw_vss_commitment_bytes() {
        let commitment = ValidatorsCommitment {
            has_aggregated_public_key: false,
            aggregated_public_key: AggregatedPublicKey::default(),
            verifiable_secret_sharing_commitment: vec![],
            validators: nonempty::nonempty![crate::Address::default()].into(),
            era_index: 0,
        };

        assert!(commitment.verifiable_secret_sharing_commitment.is_empty());
    }
}
