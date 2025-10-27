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

use crate::abi::{Gear, utils::*};
use ethexe_common::gear::*;
use gear_core::message::ReplyDetails;
use gear_core_errors::{ReplyCode, SuccessReplyReason};

//                          //
// From Rust types to alloy //
//                          //

impl From<AggregatedPublicKey> for Gear::AggregatedPublicKey {
    fn from(value: AggregatedPublicKey) -> Self {
        Self {
            x: u256_to_uint256(value.x),
            y: u256_to_uint256(value.y),
        }
    }
}

impl From<ChainCommitment> for Gear::ChainCommitment {
    fn from(value: ChainCommitment) -> Self {
        Self {
            transitions: value.transitions.into_iter().map(Into::into).collect(),
            head: value.head_announce.hash().0.into(),
        }
    }
}

impl From<CodeCommitment> for Gear::CodeCommitment {
    fn from(value: CodeCommitment) -> Self {
        Self {
            id: code_id_to_bytes32(value.id),
            valid: value.valid,
        }
    }
}

impl From<ValidatorsCommitment> for Gear::ValidatorsCommitment {
    fn from(value: ValidatorsCommitment) -> Self {
        Self {
            aggregatedPublicKey: value.aggregated_public_key.into(),
            verifiableSecretSharingCommitment: Bytes::copy_from_slice(
                &value
                    .verifiable_secret_sharing_commitment
                    .serialize()
                    .expect("Could not serialize `VerifiableSecretSharingCommitment<C>`")
                    .concat(),
            ),
            validators: value.validators.into_iter().map(|v| v.into()).collect(),
            eraIndex: Uint256::from(value.era_index),
        }
    }
}

impl From<OperatorRewardsCommitment> for Gear::OperatorRewardsCommitment {
    fn from(value: OperatorRewardsCommitment) -> Self {
        Self {
            amount: u256_to_uint256(value.amount),
            root: h256_to_bytes32(value.root),
        }
    }
}

impl From<StakerRewards> for Gear::StakerRewards {
    fn from(value: StakerRewards) -> Self {
        Self {
            vault: value.vault.0.into(),
            amount: u256_to_uint256(value.amount),
        }
    }
}

impl From<StakerRewardsCommitment> for Gear::StakerRewardsCommitment {
    fn from(value: StakerRewardsCommitment) -> Self {
        Self {
            distribution: value.distribution.into_iter().map(Into::into).collect(),
            totalAmount: u256_to_uint256(value.total_amount),
            token: value.token.0.into(),
        }
    }
}

impl From<RewardsCommitment> for Gear::RewardsCommitment {
    fn from(value: RewardsCommitment) -> Self {
        Self {
            operators: value.operators.into(),
            stakers: value.stakers.into(),
            timestamp: u64_to_uint48_lossy(value.timestamp),
        }
    }
}

impl From<BatchCommitment> for Gear::BatchCommitment {
    fn from(value: BatchCommitment) -> Self {
        Self {
            blockHash: value.block_hash.0.into(),
            blockTimestamp: u64_to_uint48_lossy(value.timestamp),
            previousCommittedBatchHash: value.previous_batch.0.into(),
            chainCommitment: value.chain_commitment.into_iter().map(Into::into).collect(),
            codeCommitments: value.code_commitments.into_iter().map(Into::into).collect(),
            rewardsCommitment: value
                .rewards_commitment
                .into_iter()
                .map(Into::into)
                .collect(),
            validatorsCommitment: value
                .validators_commitment
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

impl From<Message> for Gear::Message {
    fn from(value: Message) -> Self {
        Self {
            id: message_id_to_bytes32(value.id),
            destination: actor_id_to_address_lossy(value.destination),
            payload: value.payload.into(),
            value: value.value,
            replyDetails: value.reply_details.into(),
            call: value.call,
        }
    }
}

impl From<Gear::Timelines> for Timelines {
    fn from(value: Gear::Timelines) -> Self {
        Self {
            era: value.era.to::<u64>(),
            election: value.election.to::<u64>(),
            validation_delay: value.validationDelay.to::<u64>(),
        }
    }
}

impl From<Option<ReplyDetails>> for Gear::ReplyDetails {
    fn from(value: Option<ReplyDetails>) -> Self {
        value
            .unwrap_or(ReplyDetails::new(
                Default::default(),
                ReplyCode::Success(SuccessReplyReason::Auto),
            ))
            .into()
    }
}

impl From<ReplyDetails> for Gear::ReplyDetails {
    fn from(value: ReplyDetails) -> Self {
        let (to, code) = value.into_parts();

        Self {
            to: message_id_to_bytes32(to),
            code: code.to_bytes().into(),
        }
    }
}

impl From<StateTransition> for Gear::StateTransition {
    fn from(value: StateTransition) -> Self {
        Self {
            actorId: actor_id_to_address_lossy(value.actor_id),
            newStateHash: h256_to_bytes32(value.new_state_hash),
            exited: value.exited,
            inheritor: actor_id_to_address_lossy(value.inheritor),
            valueToReceive: value.value_to_receive,
            valueClaims: value.value_claims.into_iter().map(Into::into).collect(),
            messages: value.messages.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ValueClaim> for Gear::ValueClaim {
    fn from(value: ValueClaim) -> Self {
        Self {
            messageId: message_id_to_bytes32(value.message_id),
            destination: actor_id_to_address_lossy(value.destination),
            value: value.value,
        }
    }
}
