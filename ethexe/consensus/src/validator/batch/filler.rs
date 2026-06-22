// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use std::{mem, num::NonZero};

use super::{
    types::{BatchLimits, BatchParts, BatchSizeCounter, ValidationRejectReason},
    utils,
};

use alloy::rlp::bytes::buf::Chain;
use anyhow::Context;
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
    #[display("chain commitment already included")]
    ChainCommitmentAlreadyIncluded,
    #[display("validators commitment already included")]
    ValidatorsCommitmentAlreadyIncluded,
    #[display("rewards commitment already included")]
    RewardsCommitmentAlreadyIncluded,
}

type FillerResult = Result<(), BatchIncludeError>;

impl BatchFiller {
    pub fn new(batch_size_limit: u64) -> Self {
        Self {
            parts: BatchParts::default(),
            size_counter: BatchSizeCounter::new(batch_size_limit),
        }
    }

    pub fn into_parts(mut self) -> BatchParts {
        if let Some(chain) = &mut self.parts.chain_commitment {
            chain.transitions =
                utils::squash_transitions_by_actor(mem::take(&mut chain.transitions));
            utils::sort_transitions_by_value_to_receive(&mut chain.transitions);
        }
        self.parts
    }

    pub fn include_validators_commitment(
        &mut self,
        commitment: ValidatorsCommitment,
    ) -> FillerResult {
        if !self
            .size_counter
            .charge_for_validators_commitment(&commitment)
        {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }

        self.parts.validators_commitment = Some(commitment);
        Ok(())
    }

    pub fn include_rewards_commitment(&mut self, commitment: RewardsCommitment) -> FillerResult {
        if !self.size_counter.charge_for_rewards_commitment(&commitment) {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }

        self.parts.rewards_commitment = Some(commitment);
        Ok(())
    }

    pub fn include_code_commitment(&mut self, commitment: CodeCommitment) -> FillerResult {
        if !self.size_counter.charge_for_code_commitment(&commitment) {
            return Err(BatchIncludeError::SizeLimitExceeded);
        }

        self.parts.code_commitments.push(commitment);
        Ok(())
    }

    pub fn append_chain_commitment(&mut self, commitment: ChainCommitment) -> FillerResult {
        if let Some((existing, len)) = &mut self.parts.chain_commitment {
            let ChainCommitment {
                head,
                transitions,
                last_advanced_eth_block,
            } = commitment;

            if !self.size_counter.charge_for_transitions(&transitions) {
                return Err(BatchIncludeError::SizeLimitExceeded);
            }

            existing.head = head;
            existing.transitions.extend(transitions);
            existing.last_advanced_eth_block = last_advanced_eth_block;

            *len = len
                .checked_add(1)
                .expect("u32 chain commitment len overflow");
        } else {
            if !self.size_counter.charge_for_chain_commitment(&commitment) {
                return Err(BatchIncludeError::SizeLimitExceeded);
            }

            self.parts.chain_commitment = Some((commitment, NonZero::new(1).expect("1 != 0")));
        }

        Ok(())
    }
}

#[cfg(feature = "disable-tests")]
#[cfg(test)]
mod tests {
    use super::*;
    use alloy::sol_types::SolValue;
    use ethexe_ethereum::abi::Gear;
    use gprimitives::{CodeId, H256};

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

    /// Once the running size budget is exhausted, further code commitments
    /// must be rejected — and the rejected commitment must not leak into
    /// the accumulated parts.
    #[test]
    fn size_limit_rejects_once_budget_exhausted() {
        let first = CodeCommitment {
            id: CodeId::from([1; 32]),
            valid: true,
        };
        let encoded: Gear::CodeCommitment = first.clone().into();
        // Budget fits exactly one commitment; the second include must fail.
        let mut filler = BatchFiller::new(BatchLimits {
            batch_size_limit: encoded.abi_encoded_size() as u64,
            ..BatchLimits::default()
        });

        filler.include_code_commitment(first.clone()).unwrap();
        assert_eq!(
            filler.include_code_commitment(CodeCommitment {
                id: CodeId::from([2; 32]),
                valid: false,
            }),
            Err(BatchIncludeError::SizeLimitExceeded),
        );

        let parts = filler.into_parts();
        assert_eq!(
            parts.code_commitments,
            vec![first],
            "rejected commitment must not leak into the accumulated parts",
        );
    }
}
