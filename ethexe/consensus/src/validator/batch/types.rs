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

    pub fn charge_for_state_transitions(&mut self, transitions: &[StateTransition]) -> bool {
        let transitions: Vec<Gear::StateTransition> =
            transitions.iter().cloned().map(Into::into).collect();

        self.charge(&transitions)
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
