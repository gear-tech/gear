// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use alloy::sol_types::SolValue;
use ethexe_common::{
    Announce, Digest, HashOf,
    consensus::BatchCommitmentValidationRequest,
    gear::{
        ChainCommitment, CodeCommitment, RewardsCommitment, StateTransition, ValidatorsCommitment,
    },
};
use ethexe_ethereum::abi::Gear;
use gprimitives::CodeId;

/// Batch building limits.
#[derive(Debug, Clone)]
pub struct BatchLimits {
    /// Minimum deepness threshold to create chain commitment even if there are no transitions.
    pub chain_deepness_threshold: u32,
    /// Time limit in blocks for announce to be committed after its creation.
    pub commitment_delay_limit: u32,
    /// The maximum size of abi encoded [`ethexe_common::gear::BatchCommitment`].
    pub batch_size_limit: u64,
}

/// Tracks the remaining ABI-encoded payload budget for a candidate batch.
///
/// Each `charge_*` method subtracts the encoded size of the provided value and
/// returns `false` when adding it would exceed the maximum batch size.
#[derive(Debug, Clone)]
pub(crate) struct BatchSizeCounter(u64);

impl BatchSizeCounter {
    pub fn new(max_size: u64) -> Self {
        Self(max_size)
    }

    pub fn charge_for_validators_commitment(
        &mut self,
        commitment: &Option<ValidatorsCommitment>,
    ) -> bool {
        self.charge_optional::<ValidatorsCommitment, Gear::ValidatorsCommitment>(commitment.clone())
    }

    pub fn charge_for_rewards_commitment(
        &mut self,
        commitment: &Option<RewardsCommitment>,
    ) -> bool {
        self.charge_optional::<_, Gear::RewardsCommitment>(commitment.clone())
    }

    pub fn charge_for_chain_commitment(&mut self, commitment: &Option<ChainCommitment>) -> bool {
        self.charge_optional::<_, Gear::ChainCommitment>(commitment.clone())
    }

    /// Charges only for appended transitions after the chain commitment header
    /// has already been accounted for.
    pub fn charge_for_additional_transitions(&mut self, transitions: &[StateTransition]) -> bool {
        self.charge_many::<_, Gear::StateTransition>(transitions)
    }

    pub fn charge_for_code_commitments(&mut self, commitments: &[CodeCommitment]) -> bool {
        let commitments: Vec<Gear::CodeCommitment> =
            commitments.iter().cloned().map(Into::into).collect();

        self.charge_value(&commitments)
    }

    fn charge_optional<T, V>(&mut self, value: Option<T>) -> bool
    where
        V: SolValue,
        T: Into<V>,
    {
        let encoded: Vec<V> = value.into_iter().map(Into::into).collect();
        self.charge_value(&encoded)
    }

    fn charge_many<T, V>(&mut self, values: &[T]) -> bool
    where
        V: SolValue,
        T: Into<V> + Clone,
    {
        let mut encoded_size = 0;
        values.iter().cloned().for_each(|v| {
            encoded_size += v.into().abi_encoded_size() as u64;
        });
        self.charge(encoded_size)
    }

    fn charge_value<V: SolValue>(&mut self, value: &V) -> bool {
        self.charge(value.abi_encoded_size() as u64)
    }

    fn charge(&mut self, encoded_size: u64) -> bool {
        match self.0.checked_sub(encoded_size) {
            Some(size_left) => {
                self.0 = size_left;
                true
            }
            None => false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BatchParts {
    pub chain_commitment: Option<ChainCommitment>,
    pub code_commitments: Vec<CodeCommitment>,
    pub validators_commitment: Option<ValidatorsCommitment>,
    pub rewards_commitment: Option<RewardsCommitment>,
}

#[derive(Debug, derive_more::Display, Clone, PartialEq, Eq)]
pub enum ValidationStatus {
    #[display("accepted batch commitment with digest {_0:?}")]
    Accepted(Digest),
    #[display("rejected batch commitment request {request:?} : {reason}")]
    Rejected {
        request: BatchCommitmentValidationRequest,
        reason: ValidationRejectReason,
    },
}

#[derive(Debug, derive_more::Display, Clone, PartialEq, Eq)]
pub enum ValidationRejectReason {
    #[display("batch commitment is empty")]
    EmptyBatch,
    #[display("batch commitment request contains duplicate code ids")]
    CodesHasDuplicates,
    #[display("code id {_0} is not waiting for commitment")]
    CodeNotWaitingForCommitment(CodeId),
    #[display("code id {_0} is not processed yet")]
    CodeIsNotProcessedYet(CodeId),
    // TODO: rename this variant, because now support commitments not only for best announces.
    #[display("requested head announce {requested} is not the best announce {best}")]
    HeadAnnounceIsNotBest {
        requested: HashOf<Announce>,
        best: HashOf<Announce>,
    },
    #[display("requested head announce {_0} is not computed by this node")]
    HeadAnnounceNotComputed(HashOf<Announce>),
    #[display("cannot collect not committed predecessors for best announce {_0}")]
    BestHeadAnnounceChainInvalid(HashOf<Announce>),
    #[display(
        "received batch contains validators commitment, but it's not time for validators election yet"
    )]
    ValidatorsNotReady,
    #[display(
        "received batch contains rewards commitment, but it's not time for rewards distribution yet"
    )]
    RewardsNotReady,
    #[display("batch commitment digest mismatch: expected {expected}, found {found}")]
    BatchDigestMismatch { expected: Digest, found: Digest },
    #[display("batch size limit exceeded")]
    BatchSizeLimitExceeded,
}

#[derive(Debug, derive_more::Display, Clone, Copy, PartialEq, Eq)]
#[display("Code not found: {_0}")]
pub struct CodeNotValidatedError(pub CodeId);
