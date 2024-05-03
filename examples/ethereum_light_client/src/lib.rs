// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use core::fmt::Debug;
use alloc::{vec, vec::Vec};
use ark_bls12_381::{G1Projective as G1, G2Projective as G2};
use codec::{Decode, Encode};
use ssz_rs::{prelude::*, Deserialize, DeserializeError, Sized, Bitvector};
use superstruct::superstruct;
use tree_hash::Hash256;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

pub mod primitives;
use primitives::*;

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm;

pub type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;

pub type Address = ByteVector<20>;
pub type Bytes32 = ByteVector<32>;
pub type LogsBloom = ByteVector<256>;
pub type BLSPubKey = ByteVector<48>;
pub type SignatureBytes = ByteVector<96>;
pub type Transaction = ByteList<1_073_741_824>;

pub const SLOTS_PER_EPOCH: u64 = 32;
pub const EPOCHS_PER_SYNC_COMMITTEE: u64 = 256;

macro_rules! superstruct_ssz {
    ($type:tt) => {
        impl ssz_rs::Merkleized for $type {
            fn hash_tree_root(&mut self) -> Result<Node, MerkleizationError> {
                match self {
                    $type::Bellatrix(inner) => inner.hash_tree_root(),
                    $type::Capella(inner) => inner.hash_tree_root(),
                    $type::Deneb(inner) => inner.hash_tree_root(),
                }
            }
        }

        impl ssz_rs::Sized for $type {
            fn is_variable_size() -> bool {
                true
            }

            fn size_hint() -> usize {
                0
            }
        }

        impl ssz_rs::Serialize for $type {
            fn serialize(&self, buffer: &mut Vec<u8>) -> Result<usize, SerializeError> {
                match self {
                    $type::Bellatrix(inner) => inner.serialize(buffer),
                    $type::Capella(inner) => inner.serialize(buffer),
                    $type::Deneb(inner) => inner.serialize(buffer),
                }
            }
        }

        impl ssz_rs::Deserialize for $type {
            fn deserialize(bytes: &[u8]) -> Result<Self, DeserializeError>
            where
                Self: Sized,
            {
                paste::paste!{ [<$type Deneb>]::deserialize(bytes).map(Into::into) }
            }
        }

        impl ssz_rs::SimpleSerialize for $type {}
    };
}

#[derive(Debug, Default, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct BeaconBlock {
    pub slot: U64,
    pub proposer_index: U64,
    pub parent_root: Bytes32,
    pub state_root: Bytes32,
    pub body: BeaconBlockBody,
}

#[derive(Debug, Default, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct Eth1Data {
    deposit_root: Bytes32,
    deposit_count: U64,
    block_hash: Bytes32,
}

#[derive(Debug, Default, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
struct SignedBeaconBlockHeader {
    message: Header,
    signature: SignatureBytes,
}

#[derive(Debug, Default, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct ProposerSlashing {
    signed_header_1: SignedBeaconBlockHeader,
    signed_header_2: SignedBeaconBlockHeader,
}

#[derive(Debug, Default, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct AttesterSlashing {
    attestation_1: IndexedAttestation,
    attestation_2: IndexedAttestation,
}

#[derive(Debug, Default, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
struct IndexedAttestation {
    attesting_indices: List<U64, 2048>,
    data: AttestationData,
    signature: SignatureBytes,
}

#[derive(Debug, Default, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct Attestation {
    aggregation_bits: Bitlist<2048>,
    data: AttestationData,
    signature: SignatureBytes,
}

#[derive(Debug, Default, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
struct AttestationData {
    slot: U64,
    index: U64,
    beacon_block_root: Bytes32,
    source: Checkpoint,
    target: Checkpoint,
}

#[derive(Debug, Default, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
struct Checkpoint {
    epoch: U64,
    root: Bytes32,
}

#[derive(Debug, Default, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct Deposit {
    proof: Vector<Bytes32, 33>,
    data: DepositData,
}

#[derive(Default, Debug, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
struct DepositData {
    pubkey: BLSPubKey,
    withdrawal_credentials: Bytes32,
    amount: U64,
    signature: SignatureBytes,
}

#[derive(Debug, Default, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct SignedVoluntaryExit {
    message: VoluntaryExit,
    signature: SignatureBytes,
}

#[derive(Debug, Default, SimpleSerialize, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
struct VoluntaryExit {
    epoch: U64,
    validator_index: U64,
}

#[derive(Default, Clone, Debug, SimpleSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct SignedBlsToExecutionChange {
    message: BlsToExecutionChange,
    signature: SignatureBytes,
}

#[derive(Default, Clone, Debug, SimpleSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct BlsToExecutionChange {
    validator_index: U64,
    from_bls_pubkey: BLSPubKey,
    to_execution_address: Address,
}

#[superstruct(
    variants(Bellatrix, Capella, Deneb),
    variant_attributes(
        derive(Clone, Debug, SimpleSerialize, Default),
        cfg_attr(feature = "serde", derive(serde::Deserialize)),
        cfg_attr(feature = "serde", serde(deny_unknown_fields))
    ),
    map_ref_into(BeaconBlockBodyLight)
)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
#[derive(Debug, Clone)]
pub struct BeaconBlockBody {
    randao_reveal: SignatureBytes,
    eth1_data: Eth1Data,
    graffiti: Bytes32,
    proposer_slashings: List<ProposerSlashing, 16>,
    attester_slashings: List<AttesterSlashing, 2>,
    attestations: List<Attestation, 128>,
    deposits: List<Deposit, 16>,
    voluntary_exits: List<SignedVoluntaryExit, 16>,
    sync_aggregate: SyncAggregate,
    pub execution_payload: ExecutionPayload,
    #[superstruct(only(Capella, Deneb))]
    bls_to_execution_changes: List<SignedBlsToExecutionChange, 16>,
    #[superstruct(only(Deneb))]
    blob_kzg_commitments: List<ByteVector<48>, 4096>,
}

impl Default for BeaconBlockBody {
    fn default() -> Self {
        BeaconBlockBody::Bellatrix(BeaconBlockBodyBellatrix::default())
    }
}

superstruct_ssz!(BeaconBlockBody);

// BeaconBlockBodyLight
// @{

#[superstruct(
    variants(Bellatrix, Capella, Deneb),
    variant_attributes(
        derive(Clone, Debug, SimpleSerialize, Default)
    )
)]
#[derive(Debug, Clone)]
pub struct BeaconBlockBodyLight {
    randao_reveal: SignatureBytes,
    eth1_data: Eth1Data,
    graffiti: Bytes32,
    proposer_slashings: List<ProposerSlashing, 16>,
    attester_slashings: List<AttesterSlashing, 2>,
    attestations: List<Attestation, 128>,
    deposits: List<Deposit, 16>,
    voluntary_exits: List<SignedVoluntaryExit, 16>,
    sync_aggregate: SyncAggregate,
    pub execution_payload_header: ExecutionPayloadHeader,
    #[superstruct(only(Capella, Deneb))]
    bls_to_execution_changes_hash: Node,
    #[superstruct(only(Deneb))]
    blob_kzg_commitments_hash: Node,
}

impl Default for BeaconBlockBodyLight {
    fn default() -> Self {
        BeaconBlockBodyLight::Bellatrix(BeaconBlockBodyLightBellatrix::default())
    }
}

superstruct_ssz!(BeaconBlockBodyLight);

impl<'a> From<&'a BeaconBlockBodyBellatrix> for BeaconBlockBodyLightBellatrix {
    fn from(block_body: &'a BeaconBlockBodyBellatrix) -> Self {
        Self {
            randao_reveal: block_body.randao_reveal.clone(),
            eth1_data: block_body.eth1_data.clone(),
            graffiti: block_body.graffiti.clone(),
            proposer_slashings: block_body.proposer_slashings.clone(),
            attester_slashings: block_body.attester_slashings.clone(),
            attestations: block_body.attestations.clone(),
            deposits: block_body.deposits.clone(),
            voluntary_exits: block_body.voluntary_exits.clone(),
            sync_aggregate: block_body.sync_aggregate.clone(),
            execution_payload_header: block_body.execution_payload.to_ref().into(),
        }
    }
}
impl<'a> From<&'a BeaconBlockBodyCapella> for BeaconBlockBodyLightCapella {
    fn from(block_body: &'a BeaconBlockBodyCapella) -> Self {
        Self {
            randao_reveal: block_body.randao_reveal.clone(),
            eth1_data: block_body.eth1_data.clone(),
            graffiti: block_body.graffiti.clone(),
            proposer_slashings: block_body.proposer_slashings.clone(),
            attester_slashings: block_body.attester_slashings.clone(),
            attestations: block_body.attestations.clone(),
            deposits: block_body.deposits.clone(),
            voluntary_exits: block_body.voluntary_exits.clone(),
            sync_aggregate: block_body.sync_aggregate.clone(),
            execution_payload_header: block_body.execution_payload.to_ref().into(),
            bls_to_execution_changes_hash: block_body.bls_to_execution_changes.clone().hash_tree_root().unwrap(),
        }
    }
}

impl<'a> From<&'a BeaconBlockBodyDeneb> for BeaconBlockBodyLightDeneb {
    fn from(block_body: &'a BeaconBlockBodyDeneb) -> Self {
        Self {
            randao_reveal: block_body.randao_reveal.clone(),
            eth1_data: block_body.eth1_data.clone(),
            graffiti: block_body.graffiti.clone(),
            proposer_slashings: block_body.proposer_slashings.clone(),
            attester_slashings: block_body.attester_slashings.clone(),
            attestations: block_body.attestations.clone(),
            deposits: block_body.deposits.clone(),
            voluntary_exits: block_body.voluntary_exits.clone(),
            sync_aggregate: block_body.sync_aggregate.clone(),
            execution_payload_header: block_body.execution_payload.to_ref().into(),
            bls_to_execution_changes_hash: block_body.bls_to_execution_changes.clone().hash_tree_root().unwrap(),
            blob_kzg_commitments_hash: block_body.blob_kzg_commitments.clone().hash_tree_root().unwrap(),
        }
    }
}

impl<'a> From<BeaconBlockBodyRef<'a>> for BeaconBlockBodyLight {
    fn from(payload: BeaconBlockBodyRef<'a>) -> Self {
        map_beacon_block_body_ref_into_beacon_block_body_light!(
            &'a _,
            payload,
            |inner, cons| cons(inner.into())
        )
    }
}

// @}

// ExecutionPayload
// @{

#[derive(Default, Clone, Debug, SimpleSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct Withdrawal {
    index: U64,
    validator_index: U64,
    address: Address,
    amount: U64,
}

#[cfg(feature = "serde")]
pub fn u256_deserialize<'de, D>(deserializer: D) -> Result<U256, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let val: gstd::String = serde::Deserialize::deserialize(deserializer)?;
    let x = ethereum_types::U256::from_dec_str(&val).map_err(D::Error::custom)?;
    let mut x_bytes = [0; 32];
    x.to_little_endian(&mut x_bytes);
    Ok(U256::from_bytes_le(x_bytes))
}

#[superstruct(
    variants(Bellatrix, Capella, Deneb),
    variant_attributes(
        derive(Debug, Default, SimpleSerialize, Clone),
        cfg_attr(feature = "serde", derive(serde::Deserialize)),
        cfg_attr(feature = "serde", serde(deny_unknown_fields))
    ),
    map_ref_into(ExecutionPayloadHeader)
)]
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub struct ExecutionPayload {
    pub parent_hash: Bytes32,
    pub fee_recipient: Address,
    pub state_root: Bytes32,
    pub receipts_root: Bytes32,
    pub logs_bloom: LogsBloom,
    pub prev_randao: Bytes32,
    pub block_number: U64,
    pub gas_limit: U64,
    pub gas_used: U64,
    pub timestamp: U64,
    pub extra_data: ByteList<32>,
    #[cfg_attr(feature = "serde", serde(deserialize_with = "u256_deserialize"))]
    pub base_fee_per_gas: U256,
    pub block_hash: Bytes32,
    pub transactions: List<Transaction, 1048576>,
    #[superstruct(only(Capella, Deneb))]
    withdrawals: List<Withdrawal, 16>,
    #[superstruct(only(Deneb))]
    blob_gas_used: U64,
    #[superstruct(only(Deneb))]
    excess_blob_gas: U64,
}

impl Default for ExecutionPayload {
    fn default() -> Self {
        ExecutionPayload::Bellatrix(ExecutionPayloadBellatrix::default())
    }
}

superstruct_ssz!(ExecutionPayload);

#[superstruct(
    variants(Bellatrix, Capella, Deneb),
    variant_attributes(
        derive(Debug, Default, SimpleSerialize, Clone),
    ),
)]
#[derive(Debug, Clone)]
pub struct ExecutionPayloadHeader {
    pub parent_hash: Bytes32,
    pub fee_recipient: Address,
    pub state_root: Bytes32,
    pub receipts_root: Bytes32,
    pub logs_bloom: LogsBloom,
    pub prev_randao: Bytes32,
    pub block_number: U64,
    pub gas_limit: U64,
    pub gas_used: U64,
    pub timestamp: U64,
    pub extra_data: ByteList<32>,
    pub base_fee_per_gas: U256,
    pub block_hash: Bytes32,
    pub transactions_root: Node,
    #[superstruct(only(Capella, Deneb))]
    withdrawals_root: Node,
    #[superstruct(only(Deneb))]
    blob_gas_used: U64,
    #[superstruct(only(Deneb))]
    excess_blob_gas: U64,
}

impl Default for ExecutionPayloadHeader {
    fn default() -> Self {
        ExecutionPayloadHeader::Bellatrix(ExecutionPayloadHeaderBellatrix::default())
    }
}

superstruct_ssz!(ExecutionPayloadHeader);

impl<'a> From<&'a ExecutionPayloadBellatrix> for ExecutionPayloadHeaderBellatrix {
    fn from(payload: &'a ExecutionPayloadBellatrix) -> Self {
        Self {
            parent_hash: payload.parent_hash.clone(),
            fee_recipient: payload.fee_recipient.clone(),
            state_root: payload.state_root.clone(),
            receipts_root: payload.receipts_root.clone(),
            logs_bloom: payload.logs_bloom.clone(),
            prev_randao: payload.prev_randao.clone(),
            block_number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: payload.extra_data.clone(),
            base_fee_per_gas: payload.base_fee_per_gas.clone(),
            block_hash: payload.block_hash.clone(),
            transactions_root: payload.transactions.clone().hash_tree_root().unwrap(),
        }
    }
}
impl<'a> From<&'a ExecutionPayloadCapella> for ExecutionPayloadHeaderCapella {
    fn from(payload: &'a ExecutionPayloadCapella) -> Self {
        Self {
            parent_hash: payload.parent_hash.clone(),
            fee_recipient: payload.fee_recipient.clone(),
            state_root: payload.state_root.clone(),
            receipts_root: payload.receipts_root.clone(),
            logs_bloom: payload.logs_bloom.clone(),
            prev_randao: payload.prev_randao.clone(),
            block_number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: payload.extra_data.clone(),
            base_fee_per_gas: payload.base_fee_per_gas.clone(),
            block_hash: payload.block_hash.clone(),
            transactions_root: payload.transactions.clone().hash_tree_root().unwrap(),
            withdrawals_root: payload.withdrawals.clone().hash_tree_root().unwrap(),
        }
    }
}

impl<'a> From<&'a ExecutionPayloadDeneb> for ExecutionPayloadHeaderDeneb {
    fn from(payload: &'a ExecutionPayloadDeneb) -> Self {
        Self {
            parent_hash: payload.parent_hash.clone(),
            fee_recipient: payload.fee_recipient.clone(),
            state_root: payload.state_root.clone(),
            receipts_root: payload.receipts_root.clone(),
            logs_bloom: payload.logs_bloom.clone(),
            prev_randao: payload.prev_randao.clone(),
            block_number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: payload.extra_data.clone(),
            base_fee_per_gas: payload.base_fee_per_gas.clone(),
            block_hash: payload.block_hash.clone(),
            transactions_root: payload.transactions.clone().hash_tree_root().unwrap(),
            withdrawals_root: payload.withdrawals.clone().hash_tree_root().unwrap(),
            blob_gas_used: payload.blob_gas_used,
            excess_blob_gas: payload.excess_blob_gas,
        }
    }
}

impl<'a> From<ExecutionPayloadRef<'a>> for ExecutionPayloadHeader {
    fn from(payload: ExecutionPayloadRef<'a>) -> Self {
        map_execution_payload_ref_into_execution_payload_header!(
            &'a _,
            payload,
            |inner, cons| cons(inner.into())
        )
    }
}
// @}

// BeaconBlockHeader
#[derive(Debug, Clone, Default, SimpleSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct Header {
    pub slot: U64,
    pub proposer_index: U64,
    pub parent_root: Bytes32,
    pub state_root: Bytes32,
    pub body_root: Bytes32,
}

#[derive(Debug, Clone, Default, SimpleSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct SyncCommittee {
    pub pubkeys: ssz_rs::Vector<BLSPubKey, 512>,
    pub aggregate_pubkey: BLSPubKey,
}
#[derive(Debug, Clone, tree_hash_derive::TreeHash, Decode, Encode)]
#[codec(crate = codec)]
pub struct SyncCommittee2 {
    pub pubkeys: Array512<[u8; 48]>,
    pub aggregate_pubkey: [u8; 48],
}

#[derive(Debug, Clone, Decode, Encode)]
#[codec(crate = codec)]
pub struct Array512<T: Debug + Clone + Decode + Encode + tree_hash::TreeHash>(pub [T; 512]);

impl<T: Debug + Clone + Decode + Encode + tree_hash::TreeHash> tree_hash::TreeHash for Array512<T>
{
    fn tree_hash_type() -> tree_hash::TreeHashType {
        tree_hash::TreeHashType::Vector
    }

    fn tree_hash_packed_encoding(&self) -> tree_hash::PackedEncoding {
        unreachable!("Vector should never be packed.")
    }

    fn tree_hash_packing_factor() -> usize {
        unreachable!("Vector should never be packed.")
    }

    fn tree_hash_root(&self) -> Hash256 {
        vec_tree_hash_root::<T, 512>(&self.0)
    }
}

/// A helper function providing common functionality between the `TreeHash` implementations for
/// `FixedVector` and `VariableList`.
pub fn vec_tree_hash_root<T, const N: usize>(vec: &[T]) -> Hash256
where
    T: tree_hash::TreeHash,
{
    use tree_hash::{MerkleHasher, TreeHash, TreeHashType, BYTES_PER_CHUNK};

    match T::tree_hash_type() {
        TreeHashType::Basic => {
            let mut hasher = MerkleHasher::with_leaves(
                (N + T::tree_hash_packing_factor() - 1) / T::tree_hash_packing_factor(),
            );

            for item in vec {
                hasher
                    .write(&item.tree_hash_packed_encoding())
                    .expect("ssz_types variable vec should not contain more elements than max");
            }

            hasher
                .finish()
                .expect("ssz_types variable vec should not have a remaining buffer")
        }
        TreeHashType::Container | TreeHashType::List | TreeHashType::Vector => {
            let mut hasher = MerkleHasher::with_leaves(N);

            for item in vec {
                hasher
                    .write(item.tree_hash_root().as_bytes())
                    .expect("ssz_types vec should not contain more elements than max");
            }

            hasher
                .finish()
                .expect("ssz_types vec should not have a remaining buffer")
        }
    }
}

#[derive(Debug, Clone, Decode, Encode)]
#[codec(crate = codec)]
pub struct Init {
    pub last_checkpoint: [u8; 32],
    pub pub_keys: ArkScale<Vec<G1>>,
    pub current_sync_committee: SyncCommittee2,
    // all next fields are ssz_rs serialized
    pub finalized_header: Vec<u8>,
    pub current_sync_committee_branch: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, Default, SimpleSerialize)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub struct SyncAggregate {
    pub sync_committee_bits: Bitvector<512>,
    pub sync_committee_signature: SignatureBytes,
}

#[derive(Debug, Clone, Default, SimpleSerialize)]
pub struct Update {
    pub attested_header: Header,
    pub sync_aggregate: SyncAggregate,
    pub next_sync_committee: Option<SyncCommittee>,
    pub finalized_header: Header,
}

#[derive(Debug, Clone, Decode, Encode)]
#[codec(crate = codec)]
pub enum Handle {
    Update {
        // ssz_rs serialized Update struct
        update: Vec<u8>,
        signature_slot: u64,
        // serialized without compression
        sync_committee_signature: ArkScale<G2>,
        next_sync_committee: Option<ArkScale<Vec<G1>>>,
        next_sync_committee_branch: Option<Vec<[u8; 32]>>,
        finality_branch: Vec<[u8; 32]>,
    },
    BeaconBlockBody {
        // ssz_rs serialized
        beacon_block_body_light: Vec<u8>,
        // ssz_rs serialized
        transaction_hashes: Vec<u8>,
    }
}

pub fn calc_sync_period(slot: u64) -> u64 {
    let epoch = slot / SLOTS_PER_EPOCH;

    epoch / EPOCHS_PER_SYNC_COMMITTEE
}
