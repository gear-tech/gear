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

use std::iter::chain;

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
use parity_scale_codec::Encode;

// We assume that maximum Ethereum transaction's payload is 100 KB.
const MAX_BATCH_SIZE: u64 = 100 * 1024;

/// Сделать подсчет для скольки программ мы будем комитить транзишены.

/// This struct represents the limits for batch.
#[derive(Debug, Clone)]
pub struct BatchLimits {
    /// Minimum deepness threshold to create chain commitment even if there are no transitions.
    pub chain_deepness_threshold: u32,
    /// Time limit in blocks for announce to be committed after its creation.
    pub commitment_delay_limit: u32,
}

/// The gas weights for [`BatchCommitment`](ethexe_common::gear::BatchCommitment) parts.
/// This weight are using for batch building in [`BatchGasCounter`].  #[derive(Debug, Clone)]
#[derive(Debug, Clone)]
pub struct BatchGasWeights {
    pub max_gas_per_batch: u64,
    pub validators_commitment_gas: u64,
    pub rewards_commitment_gas: u64,
    pub state_transition_gas: u64,
    pub code_commitment_gas: u64,
}

// TODO: remove default from here
impl Default for BatchGasWeights {
    fn default() -> Self {
        Self {
            max_gas_per_batch: 1_000_000_000,
            validators_commitment_gas: 100,
            rewards_commitment_gas: 30,
            state_transition_gas: 50,
            code_commitment_gas: 10,
        }
    }
}

// BatchSizeCounter (max_codes_limit * 200, max_transitions: 10)

// If we will have gas measures in (eth):
// validators_commitment gas: 10k gas
// single transition gas: 5k gas
// single code commitment: 1k gas
//
// And we will have:
// maximum batch tx gas: 500k gas.

/// Size counter for batch commitment. Track the size of data included into batch.
#[derive(Debug, Clone)]
pub(crate) struct BatchGasCounter {
    gas_left: u64,
    gas_weights: BatchGasWeights,
}

impl BatchGasCounter {
    /// Creates new batch gas counter.
    pub fn new(gas_weights: BatchGasWeights) -> Self {
        Self {
            gas_left: gas_weights.max_gas_per_batch,
            gas_weights,
        }
    }

    pub fn gas_left(&self) -> u64 {
        self.gas_left
    }
    pub fn gas_weights(&self) -> &BatchGasWeights {
        &self.gas_weights
    }

    pub fn charge_for_validators_commitment(&mut self) -> bool {
        self.charge_inner(self.gas_weights.validators_commitment_gas)
    }

    pub fn charge_for_rewards_commitment(&mut self) -> bool {
        self.charge_inner(self.gas_weights.rewards_commitment_gas)
    }

    pub fn charge_for_transitions(&mut self, transitions_len: u64) -> bool {
        match self
            .gas_weights
            .state_transition_gas
            .checked_mul(transitions_len)
        {
            Some(transitions_gas) => self.charge_inner(transitions_gas),
            None => false,
        }
    }

    pub fn charge_for_code_commitments(&mut self, commitments_len: u64) -> bool {
        match self
            .gas_weights
            .code_commitment_gas
            .checked_mul(commitments_len)
        {
            Some(code_commitments_gas) => self.charge_inner(code_commitments_gas),
            None => false,
        }
    }

    // Inner function for correct gas charging.
    fn charge_inner(&mut self, value: u64) -> bool {
        match self.gas_left.checked_sub(value) {
            Some(gas) => {
                self.gas_left = gas;
                true
            }
            None => false,
        }
    }
}

// TODO !!!: For batch size counter need to write a proptest for correctness batch size counting.
#[derive(Debug, Clone)]
pub(crate) struct BatchSizeCounter(u64);

impl BatchSizeCounter {
    pub fn new() -> Self {
        Self(MAX_BATCH_SIZE)
    }

    pub fn size_left(&self) -> u64 {
        self.0
    }

    pub fn charge_for_validators_commitment(
        &mut self,
        commitment: &Option<ValidatorsCommitment>,
    ) -> bool {
        let commitment: Vec<Gear::ValidatorsCommitment> =
            commitment.iter().cloned().map(Into::into).collect();

        self.charge(&commitment)
    }

    pub fn charge_for_rewards_commitment(
        &mut self,
        commitment: &Option<RewardsCommitment>,
    ) -> bool {
        let commitment: Vec<Gear::RewardsCommitment> =
            commitment.iter().cloned().map(Into::into).collect();

        self.charge(&commitment)
    }

    pub fn charge_for_chain_commitment(&mut self, commitment: &Option<ChainCommitment>) -> bool {
        let commitment: Vec<Gear::ChainCommitment> =
            commitment.iter().cloned().map(Into::into).collect();

        self.charge(&commitment)
    }

    /// Charges for the size of additional transitions when size for [`ChainCommitment`] already charged.
    ///
    /// This functional just charge only for state transitions abi encoding, without any additional fields like length and data pointer.
    pub fn charge_for_additional_transitions(&mut self, transitions: &[StateTransition]) -> bool {
        for transition in transitions.iter().cloned() {
            let tr: Gear::StateTransition = transition.into();
            if !self.charge(&tr) {
                return false;
            }
        }

        true
    }

    pub fn charge_for_code_commitments(&mut self, commitments: &[CodeCommitment]) -> bool {
        let commitments: Vec<Gear::CodeCommitment> =
            commitments.iter().cloned().map(Into::into).collect();

        self.charge(&commitments)
    }

    fn charge<V: SolValue>(&mut self, value: &V) -> bool {
        let encoded_size = value.abi_encoded_size();

        match self.0.checked_sub(encoded_size as u64) {
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

#[derive(Debug, derive_more::Display, Clone, Copy, PartialEq, Eq)]
pub enum BatchIncludeError {
    #[display("batch gas limit exceeded")]
    GasLimitExceeded,
    #[display("batch size limit exceeded")]
    SizeLimitExceeded,
}

impl From<BatchIncludeError> for ValidationRejectReason {
    fn from(value: BatchIncludeError) -> Self {
        match value {
            BatchIncludeError::GasLimitExceeded => Self::BatchGasLimitExceeded,
            BatchIncludeError::SizeLimitExceeded => Self::BatchSizeLimitExceeded,
        }
    }
}

/// Helper that owns the mutable batch assembly state while the manager
/// decides what candidate parts should be considered for inclusion.
#[derive(Debug, Clone)]
pub struct BatchFiller {
    parts: BatchParts,
    gas_counter: BatchGasCounter,
    size_counter: BatchSizeCounter,
}

type FillerResult = Result<(), BatchIncludeError>;

impl BatchFiller {
    pub fn new(gas_weights: BatchGasWeights) -> Self {
        Self {
            parts: BatchParts::default(),
            gas_counter: BatchGasCounter::new(gas_weights),
            size_counter: BatchSizeCounter::new(),
        }
    }

    pub fn parts(&self) -> &BatchParts {
        &self.parts
    }

    pub fn into_parts(self) -> BatchParts {
        self.parts
    }

    pub fn include_validators_commitment(
        &mut self,
        commitment: Option<ValidatorsCommitment>,
    ) -> FillerResult {
        if !self.gas_counter.charge_for_validators_commitment() {
            return Err(BatchIncludeError::GasLimitExceeded);
        }

        if !self
            .size_counter
            .charge_for_validators_commitment(&commitment)
        {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }

        self.parts.validators_commitment = commitment;
        Ok(())
    }

    pub fn include_rewards_commitment(
        &mut self,
        commitment: Option<RewardsCommitment>,
    ) -> FillerResult {
        if !self.gas_counter.charge_for_rewards_commitment() {
            return Err(BatchIncludeError::GasLimitExceeded);
        }
        if !self.size_counter.charge_for_rewards_commitment(&commitment) {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }

        self.parts.rewards_commitment = commitment;
        Ok(())
    }

    pub fn include_code_commitments(&mut self, commitments: Vec<CodeCommitment>) -> FillerResult {
        if !self
            .gas_counter
            .charge_for_code_commitments(commitments.len() as u64)
        {
            return Err(BatchIncludeError::GasLimitExceeded);
        }
        if !self.size_counter.charge_for_code_commitments(&commitments) {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }

        self.parts.code_commitments.extend(commitments);
        Ok(())
    }

    pub fn include_chain_commitment(&mut self, commitment: ChainCommitment) -> FillerResult {
        match self.parts.chain_commitment {
            Some(ref mut chain_commitment) => {
                if !self
                    .gas_counter
                    .charge_for_transitions(commitment.transitions.len() as u64)
                {
                    return Err(BatchIncludeError::GasLimitExceeded);
                }
                if !self
                    .size_counter
                    .charge_for_additional_transitions(&commitment.transitions)
                {
                    return Err(BatchIncludeError::SizeLimitExceeded);
                }
                chain_commitment.head_announce = commitment.head_announce;
                chain_commitment.transitions.extend(commitment.transitions);
            }
            None if !commitment.transitions.is_empty() => {
                // if !self
                //     .gas_counter
                //     .charge_for_transitions(commitment.transitions.len() as u64)
                // {
                //     return Err(BatchIncludeError::GasLimitExceeded);
                // }

                let commitment = Some(commitment);
                if !self.size_counter.charge_for_chain_commitment(&commitment) {
                    return Err(BatchIncludeError::SizeLimitExceeded);
                }
                self.parts.chain_commitment = commitment;
            }
            None => {}
        }
        Ok(())
    }

    pub fn include_chain_and_codes_commitments(
        &mut self,
        chain_commitment: ChainCommitment,
        code_commitments: Vec<CodeCommitment>,
    ) -> FillerResult {
        // This is wrong implementation, should be fixed.
        self.include_chain_commitment(chain_commitment)?;
        self.include_code_commitments(code_commitments)
    }
}

// TODO: maybe this upper
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
    #[display("batch gas limit exceeded")]
    BatchGasLimitExceeded,
    #[display("batch size limit exceeded")]
    BatchSizeLimitExceeded,
}

#[derive(Debug, derive_more::Display, Clone, Copy, PartialEq, Eq)]
#[display("Code not found: {_0}")]
pub struct CodeNotValidatedError(pub CodeId);
