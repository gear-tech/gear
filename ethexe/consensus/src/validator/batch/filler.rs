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

use super::types::{BatchLimits, BatchParts, BatchSizeCounter, ValidationRejectReason};

use ethexe_common::gear::{
    ChainCommitment, CodeCommitment, RewardsCommitment, ValidatorsCommitment,
};

/// Helper that owns the mutable batch assembly state while the manager
/// decides what candidate parts should be considered for inclusion.
#[derive(Debug, Clone)]
pub struct BatchFiller {
    parts: BatchParts,
    limits: BatchLimits,
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
            limits,
            size_counter: BatchSizeCounter::new(),
        }
    }

    pub fn into_parts(self) -> BatchParts {
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

    pub fn include_code_commitments(&mut self, commitments: Vec<CodeCommitment>) -> FillerResult {
        if !self.size_counter.charge_for_code_commitments(&commitments) {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }

        // TODO: fix this behavior
        // self.parts.code_commitments.extend(commitments);
        self.parts.code_commitments = commitments;
        Ok(())
    }

    pub fn include_chain_commitment(
        &mut self,
        commitment: ChainCommitment,
        deepness: u32,
    ) -> FillerResult {
        match self.parts.chain_commitment.as_mut() {
            Some(chain_commitment) => {
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

    pub fn include_chain_and_codes_commitments(
        &mut self,
        chain_commitment: ChainCommitment,
        deepness: u32,
        code_commitments: Vec<CodeCommitment>,
    ) -> FillerResult {
        if !self.can_include_chain_and_codes_commitments(
            &chain_commitment,
            deepness,
            &code_commitments,
        ) {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }

        self.include_chain_commitment(chain_commitment, deepness)?;
        self.include_code_commitments(code_commitments)
    }

    fn can_include_chain_and_codes_commitments(
        &self,
        chain_commitment: &ChainCommitment,
        deepness: u32,
        code_commitments: &[CodeCommitment],
    ) -> bool {
        let mut size_counter = self.size_counter.clone();

        match self.parts.chain_commitment.is_some() {
            true => {
                if !size_counter.charge_for_additional_transitions(&chain_commitment.transitions) {
                    return false;
                }
            }
            false => {
                if !self.should_include_chain_commitment(chain_commitment, deepness) {
                    return size_counter.charge_for_code_commitments(code_commitments);
                }

                if !size_counter.charge_for_chain_commitment(&Some(chain_commitment.clone())) {
                    return false;
                }
            }
        }

        size_counter.charge_for_code_commitments(code_commitments)
    }

    fn should_include_chain_commitment(&self, commitment: &ChainCommitment, deepness: u32) -> bool {
        !commitment.transitions.is_empty() || deepness > self.limits.chain_deepness_threshold
    }
}
