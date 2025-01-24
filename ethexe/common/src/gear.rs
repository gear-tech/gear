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

use alloc::vec::Vec;
use gear_core::message::{ReplyDetails, StoredMessage};
use gprimitives::{ActorId, CodeId, MessageId, H256, U256};
use parity_scale_codec::{Decode, Encode};

// TODO: support query from router.
pub const COMPUTATION_THRESHOLD: u64 = 2_500_000_000;
pub const SIGNING_THRESHOLD_PERCENTAGE: u16 = 6666;
pub const WVARA_PER_SECOND: u128 = 10_000_000_000_000;

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct AggregatedPublicKey {
    pub x: U256,
    pub y: U256,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct VerifyingShare {
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

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct CodeCommitment {
    pub id: CodeId,
    pub valid: bool,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct ValidatorsCommitment {
    pub aggregated_public_key: AggregatedPublicKey,
    pub verifying_shares: Vec<VerifyingShare>,
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
pub struct Message {
    pub id: MessageId,
    pub destination: ActorId,
    pub payload: Vec<u8>,
    pub value: u128,
    pub reply_details: Option<ReplyDetails>,
}

impl From<StoredMessage> for Message {
    fn from(value: StoredMessage) -> Self {
        let (id, _source, destination, payload, value, details) = value.into_parts();
        Self {
            id,
            destination,
            payload: payload.into_vec(),
            value,
            reply_details: details.and_then(|v| v.to_reply_details()),
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
pub struct StateTransition {
    pub actor_id: ActorId,
    pub new_state_hash: H256,
    pub inheritor: ActorId,
    pub value_to_receive: u128,
    pub value_claims: Vec<ValueClaim>,
    pub messages: Vec<Message>,
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct ValidationSettings {
    pub signing_threshold_percentage: u16,
    pub validators: Vec<ActorId>,
    // flatten mapping of validators ActorId => bool
}

#[derive(Clone, Debug, Default, Encode, Decode, PartialEq, Eq)]
pub struct ValueClaim {
    pub message_id: MessageId,
    pub destination: ActorId,
    pub value: u128,
}
