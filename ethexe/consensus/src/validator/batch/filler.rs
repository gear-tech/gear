// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::types::{BatchLimits, BatchParts, BatchSizeCounter, ValidationRejectReason};

use ethexe_common::gear::{
    ChainCommitment, CodeCommitment, RewardsCommitment, ValidatorsCommitment,
};

// TODO #5356: squash transitions before charging size so repeated actors are
// counted against the actual committed payload rather than the pre-squash input.
/// Stateful helper used by [`BatchCommitmentManager`](super::manager::BatchCommitmentManager)
/// to assemble a candidate batch commitment under protocol size and deepness limits.
///
/// The manager decides which commitments are eligible, while `BatchFiller`
/// tracks the accumulated parts and rejects additions that would exceed the
/// batch payload budget.
#[derive(Debug, Clone)]
pub struct BatchFiller {
    /// Parts accumulated for the candidate batch being assembled.
    parts: BatchParts,
    /// Protocol limits that decide whether candidate parts may be included.
    limits: BatchLimits,
    /// Running payload budget for the ABI-encoded batch commitment.
    size_counter: BatchSizeCounter,
}

#[derive(Debug, derive_more::Display, Clone, Copy, PartialEq, Eq)]
pub enum BatchIncludeError {
    #[display("batch size limit exceeded")]
    SizeLimitExceeded,
}

impl From<BatchIncludeError> for ValidationRejectReason {
    fn from(value: BatchIncludeError) -> Self {
        match value {
            BatchIncludeError::SizeLimitExceeded => Self::BatchSizeLimitExceeded,
        }
    }
}

type FillerResult = Result<(), BatchIncludeError>;

impl BatchFiller {
    pub fn new(limits: BatchLimits) -> Self {
        Self {
            parts: BatchParts::default(),
            size_counter: BatchSizeCounter::new(limits.batch_size_limit),
            limits,
        }
    }

    pub fn into_parts(mut self) -> BatchParts {
        if let Some(chain) = &mut self.parts.chain_commitment {
            chain.transitions =
                super::utils::squash_transitions_by_actor(std::mem::take(&mut chain.transitions));
            super::utils::sort_transitions_by_value_to_receive(&mut chain.transitions);
        }
        self.parts
    }

    pub fn include_validators_commitment(
        &mut self,
        commitment: ValidatorsCommitment,
    ) -> FillerResult {
        let commitment = Some(commitment);
        if !self
            .size_counter
            .charge_for_validators_commitment(&commitment)
        {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }

        self.parts.validators_commitment = commitment;
        Ok(())
    }

    pub fn include_rewards_commitment(&mut self, commitment: RewardsCommitment) -> FillerResult {
        let commitment = Some(commitment);
        if !self.size_counter.charge_for_rewards_commitment(&commitment) {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }

        self.parts.rewards_commitment = commitment;
        Ok(())
    }

    pub fn include_code_commitment(&mut self, commitment: CodeCommitment) -> FillerResult {
        if !self.size_counter.charge_for_code_commitment(&commitment) {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }

        self.parts.code_commitments.push(commitment);
        Ok(())
    }

    pub fn include_chain_commitment(
        &mut self,
        commitment: ChainCommitment,
        deepness: u32,
    ) -> FillerResult {
        match self.parts.chain_commitment.as_mut() {
            Some(chain_commitment) => {
                // Once the chain header is present, only appended transitions consume extra space.
                if !self
                    .size_counter
                    .charge_for_additional_transitions(&commitment.transitions)
                {
                    return Err(BatchIncludeError::SizeLimitExceeded);
                }
                chain_commitment.head_announce = commitment.head_announce;
                chain_commitment.transitions.extend(commitment.transitions);
            }
            None => {
                // NOTE: Empty transition chains are skipped until they become old enough to force inclusion.
                if !self.should_include_chain_commitment(&commitment, deepness) {
                    return Ok(());
                }

                let commitment = Some(commitment);
                if !self.size_counter.charge_for_chain_commitment(&commitment) {
                    return Err(BatchIncludeError::SizeLimitExceeded);
                }
                self.parts.chain_commitment = commitment;
            }
        }
        Ok(())
    }

    fn should_include_chain_commitment(&self, commitment: &ChainCommitment, deepness: u32) -> bool {
        // A deep enough chain must eventually be committed even if it carries no transitions.
        !commitment.transitions.is_empty() || deepness + 1 > self.limits.chain_deepness_threshold
    }
}
