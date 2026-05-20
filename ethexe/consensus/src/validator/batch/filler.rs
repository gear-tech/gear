// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::types::{BatchLimits, BatchParts, BatchSizeCounter, ValidationRejectReason};

use ethexe_common::gear::{
    ChainCommitment, CodeCommitment, RewardsCommitment, ValidatorsCommitment,
};

// TODO #5356: squash transitions before charging size so repeated actors are
// counted against the actual committed payload rather than the pre-squash input.
/// Stateful helper used by [`BatchCommitmentManager`](super::manager::BatchCommitmentManager)
/// to assemble a candidate batch commitment under protocol size limits.
///
/// The manager decides which commitments are eligible, while `BatchFiller`
/// tracks the accumulated parts and rejects additions that would exceed the
/// batch payload budget.
#[derive(Debug, Clone)]
pub struct BatchFiller {
    /// Parts accumulated for the candidate batch being assembled.
    parts: BatchParts,
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

    pub fn has_chain_commitment(&self) -> bool {
        self.parts.chain_commitment.is_some()
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

    /// Probe whether a hypothetical chain commitment with `transitions` would
    /// still fit the remaining batch budget. Used by the producer to grow the
    /// chain commitment one MB at a time and stop *before* the size limit is
    /// breached, so the call to [`Self::include_chain_commitment`] is
    /// guaranteed to succeed.
    pub fn would_fit_chain_commitment(&self, candidate: &ChainCommitment) -> bool {
        let mut probe = self.size_counter.clone();
        probe.charge_for_chain_commitment(&Some(candidate.clone()))
    }

    pub fn include_code_commitment(&mut self, commitment: CodeCommitment) -> FillerResult {
        if !self.size_counter.charge_for_code_commitment(&commitment) {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }

        self.parts.code_commitments.push(commitment);
        Ok(())
    }

    /// Include a freshly aggregated chain commitment in the batch.
    ///
    /// A commitment with neither transitions nor an Ethereum-anchor
    /// advance carries no payload and is dropped — the next coordinator
    /// round will re-walk and pick up whatever has finalized since.
    /// Empty-transitions checkpoints **with** a non-zero
    /// `last_advanced_eth_block` are kept: they exist specifically to push
    /// the on-chain Ethereum anchor forward during long quiet stretches.
    pub fn include_chain_commitment(&mut self, commitment: ChainCommitment) -> FillerResult {
        if commitment.transitions.is_empty() && commitment.last_advanced_eth_block.is_zero() {
            return Ok(());
        }

        let commitment = Some(commitment);
        if !self.size_counter.charge_for_chain_commitment(&commitment) {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }
        self.parts.chain_commitment = commitment;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gprimitives::H256;

    /// Checkpoint chain commitments carry empty transitions but a
    /// non-zero `last_advanced_eth_block` — they exist *specifically*
    /// to push the on-chain Ethereum anchor forward when the chain has
    /// been quiet for a long stretch. The filler must keep them.
    #[test]
    fn include_chain_commitment_keeps_checkpoint_with_no_transitions() {
        let mut filler = BatchFiller::new(BatchLimits::default());
        let checkpoint = ChainCommitment {
            head: H256::from_low_u64_be(0xC0DE),
            transitions: Vec::new(),
            last_advanced_eth_block: H256::from_low_u64_be(0xEB),
        };

        filler.include_chain_commitment(checkpoint).unwrap();
        assert!(
            filler.has_chain_commitment(),
            "checkpoint with empty transitions but a non-zero advanced anchor must \
             be retained — dropping it strands the Ethereum-side anchor advance"
        );
    }
}
