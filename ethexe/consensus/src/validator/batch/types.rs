// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use alloy::sol_types::SolValue;
use core::num::NonZero;
use ethexe_common::{
    DEFAULT_COMMITMENT_DELAY_LIMIT, Digest,
    consensus::{BatchCommitmentValidationRequest, DEFAULT_BATCH_SIZE_LIMIT},
    gear::{ChainCommitment, CodeCommitment, RewardsCommitment, ValidatorsCommitment},
};
use ethexe_ethereum::abi::Gear;
use gprimitives::{CodeId, H256};

/// Batch building limits.
#[derive(Debug, Clone)]
pub struct BatchLimits {
    /// Coordinator-local: how many Ethereum blocks the resulting
    /// `BatchCommitment` stays valid past its target block. Encoded into
    /// `BatchCommitment::expiry` (also `u8`). Set freely per-coordinator.
    pub commitment_delay_limit: NonZero<u8>,
    pub batch_size_limit: u64,
    /// Force a checkpoint chain commitment when the producer's view of
    /// `last_advanced_eth_block` is more than this many blocks ahead of the
    /// last committed advanced block.
    pub uncommitted_chain_len_threshold: NonZero<u32>,
}

impl Default for BatchLimits {
    fn default() -> Self {
        BatchLimits {
            commitment_delay_limit: DEFAULT_COMMITMENT_DELAY_LIMIT,
            batch_size_limit: DEFAULT_BATCH_SIZE_LIMIT,
            uncommitted_chain_len_threshold: NonZero::new(500).expect("500 != 0"),
        }
    }
}

/// Tracks an approximate remaining ABI payload budget for a candidate batch.
///
/// This counter is intentionally conservative but not exact: it charges the
/// variable-size payloads of batch parts and relies on `batch_size_limit`
/// having reserved slack for ABI layout overhead such as the top-level tuple
/// head, dynamic offsets, and length words.
///
/// In other words, this is not a byte-perfect `Gear::BatchCommitment`
/// encoder. The configured limit must include enough headroom to cover the
/// difference between this approximation and the final ABI encoding.
///
/// Each `charge_*` method subtracts the estimated encoded size of the provided
/// value and returns `false` when adding it would exceed the maximum batch size.
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

    pub fn charge_for_code_commitment(&mut self, commitment: &CodeCommitment) -> bool {
        let commitment: Gear::CodeCommitment = commitment.clone().into();

        self.charge_value(&commitment)
    }

    fn charge_optional<T, V>(&mut self, value: Option<T>) -> bool
    where
        V: SolValue,
        T: Into<V>,
    {
        let encoded: Vec<V> = value.into_iter().map(Into::into).collect();
        self.charge_value(&encoded)
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
    #[display("requested head MB {_0} is not finalized locally")]
    HeadMbNotFinalized(H256),
    #[display("requested head MB {_0} is at or below last committed MB")]
    HeadMbAlreadyCommitted(H256),
    #[display("requested head MB {_0} is not computed by this node")]
    HeadMbNotComputed(H256),
    #[display(
        "latest finalized MB advance {_0} is not on the canonical chain ending at the current head"
    )]
    LatestFinalizedAdvanceNotCanonical(H256),
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
    #[display("batch size exceeded the maximum size limit")]
    BatchSizeLimitExceeded,
}

#[derive(Debug, derive_more::Display, Clone, Copy, PartialEq, Eq)]
#[display("Code not found: {_0}")]
pub struct CodeNotValidatedError(pub CodeId);
