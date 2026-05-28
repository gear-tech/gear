// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::types::{BatchLimits, CodeNotValidatedError, ValidationRejectReason, ValidationStatus};
use crate::validator::{
    batch::{filler::BatchFiller, types::BatchParts, utils},
    core::{ElectionRequest, MiddlewareWrapper},
};

use alloy::sol_types::SolValue;
use anyhow::{Context as _, Result, anyhow, bail};
use ethexe_common::{
    SimpleBlockData, ToDigest,
    consensus::BatchCommitmentValidationRequest,
    db::{BlockMetaStorageRO, ConfigStorageRO, GlobalsStorageRO, MbStorageRO, OnChainStorageRO},
    gear::{BatchCommitment, ChainCommitment, RewardsCommitment, ValidatorsCommitment},
};
use ethexe_db::Database;
use ethexe_ethereum::abi::Gear;
use gprimitives::H256;
use hashbrown::HashSet;

#[derive(derive_more::Debug, Clone)]
pub struct BatchCommitmentManager {
    /// Limits for batch building and verifying
    limits: BatchLimits,
    /// The ethexe database instance.
    #[debug(skip)]
    db: Database,
    /// The ethexe middleware for validators election.
    #[debug(skip)]
    middleware: MiddlewareWrapper,
}

impl BatchCommitmentManager {
    /// Creates a new instance of batch commitment manager.
    pub fn new(limits: BatchLimits, db: Database, middleware: MiddlewareWrapper) -> Self {
        Self {
            limits,
            db,
            middleware,
        }
    }

    /// Coordinator-side batch builder. Walks `[last_committed_mb..latest_finalized_mb]`
    /// and pairs the chain piece with validators / rewards / code commitments.
    /// Returns `Ok(None)` when there's nothing to commit.
    pub async fn create_batch_commitment(
        self,
        block: SimpleBlockData,
    ) -> Result<Option<BatchCommitment>> {
        let mut batch_filler = BatchFiller::new(self.limits.clone());

        if let Some(validators_commitment) = self.aggregate_validators_commitment(&block).await?
            && let Err(err) = batch_filler.include_validators_commitment(validators_commitment)
        {
            bail!("failed to include validators commitment into batch, err={err}")
        }

        if let Some(rewards_commitment) = self.aggregate_rewards_commitment(&block).await?
            && let Err(err) = batch_filler.include_rewards_commitment(rewards_commitment)
        {
            bail!("failed to include rewards commitment into batch, err={err}")
        }

        // State transitions before code commitments.
        let latest_finalized_mb = self.db.globals().latest_finalized_mb_hash;
        if !latest_finalized_mb.is_zero() {
            let latest_advanced = self.db.mb_meta(latest_finalized_mb).last_advanced_eb;
            if !crate::utils::is_eth_block_canonical_to(&self.db, latest_advanced, block.hash)? {
                // Eth reorged deeper than canonical_quarantine past a finalized
                // MB; commitments stall until Eth reverts.
                tracing::error!(
                    %latest_finalized_mb,
                    %latest_advanced,
                    block = %block.hash,
                    "coordinator: latest finalized MB advanced to a non-canonical Eth block — \
                     refusing to build batch (commitments to Eth are now blocked until recovery)"
                );
                return Ok(None);
            }

            // `try_include_chain_commitment` is lenient; only DB-invariant errors propagate.
            super::utils::try_include_chain_commitment(
                &self.db,
                block.hash,
                latest_finalized_mb,
                &mut batch_filler,
            )?;

            // Checkpoint: if no chain commitment fits but the producer's
            // `last_advanced_eth_block` is far ahead of `last_committed_eb`,
            // emit an empty chain commitment that just bumps the on-chain anchor.
            if !batch_filler.has_chain_commitment() {
                super::utils::try_include_checkpoint_chain_commitment(
                    &self.db,
                    block.hash,
                    latest_finalized_mb,
                    self.limits.uncommitted_chain_len_threshold,
                    &mut batch_filler,
                )?;
            }
        }

        let queue = self.db.block_meta(block.hash).codes_queue.ok_or_else(|| {
            anyhow!(
                "Computed block {} codes queue is not in storage",
                block.hash
            )
        })?;
        let code_commitments = super::utils::aggregate_code_commitments(&self.db, queue, false)
            .expect("not errors because, fail_if_not_found is set to false");

        for commitment in code_commitments {
            if let Err(err) = batch_filler.include_code_commitment(commitment) {
                tracing::trace!(
                    "failed to include all code commitments into batch, because of error={err}"
                );
                break;
            }
        }

        super::utils::create_batch_commitment(
            &self.db,
            &block,
            batch_filler.into_parts(),
            self.limits.commitment_delay_limit,
        )
    }

    /// Participant: re-derive the coordinator's batch and return whether digests agree.
    /// Drops the signature (Rejected) on chain mismatch instead of erroring.
    pub async fn validate_batch_commitment(
        self,
        block: SimpleBlockData,
        request: BatchCommitmentValidationRequest,
    ) -> Result<ValidationStatus> {
        let &BatchCommitmentValidationRequest {
            digest,
            head,
            ref codes,
            validators,
            rewards,
        } = &request;
        let mut batch_parts = BatchParts::default();

        if crate::utils::has_duplicates(codes.as_slice()) {
            return Ok(ValidationStatus::Rejected {
                request,
                reason: ValidationRejectReason::CodesHasDuplicates,
            });
        }

        if validators {
            match self.aggregate_validators_commitment(&block).await? {
                Some(commitment) => batch_parts.validators_commitment = Some(commitment),
                None => {
                    return Ok(ValidationStatus::Rejected {
                        request,
                        reason: ValidationRejectReason::ValidatorsNotReady,
                    });
                }
            }
        }

        if rewards {
            match self.aggregate_rewards_commitment(&block).await? {
                Some(commitment) => batch_parts.rewards_commitment = Some(commitment),
                None => {
                    return Ok(ValidationStatus::Rejected {
                        request,
                        reason: ValidationRejectReason::RewardsNotReady,
                    });
                }
            }
        }

        let waiting_codes = self
            .db
            .block_meta(block.hash)
            .codes_queue
            .ok_or_else(|| anyhow!("codes queue not found for block={}", block.hash))?
            .into_iter()
            .collect::<HashSet<_>>();

        if let Some(&code_id) = codes.iter().find(|&id| !waiting_codes.contains(id)) {
            return Ok(ValidationStatus::Rejected {
                request,
                reason: ValidationRejectReason::CodeNotWaitingForCommitment(code_id),
            });
        }

        match super::utils::aggregate_code_commitments(&self.db, codes.iter().copied(), true) {
            Ok(commitments) => batch_parts.code_commitments = commitments,
            Err(CodeNotValidatedError(code_id)) => {
                return Ok(ValidationStatus::Rejected {
                    request,
                    reason: ValidationRejectReason::CodeIsNotProcessedYet(code_id),
                });
            }
        };

        if let Some(head_mb) = head {
            // Mirror the coordinator-side guard: refuse to sign anything if our
            // own `latest_finalized_mb` advanced to a non-canonical Eth block
            // (deep Eth reorg past quarantine). The coordinator's advance must
            // also be canonical here for the batch to ever land.
            let local_latest_finalized = self.db.globals().latest_finalized_mb_hash;
            if !local_latest_finalized.is_zero() {
                let latest_advanced = self.db.mb_meta(local_latest_finalized).last_advanced_eb;
                if !crate::utils::is_eth_block_canonical_to(&self.db, latest_advanced, block.hash)?
                {
                    return Ok(ValidationStatus::Rejected {
                        request,
                        reason: ValidationRejectReason::LatestFinalizedAdvanceNotCanonical(
                            latest_advanced,
                        ),
                    });
                }
            }

            // BFT-safety: any two finalized MBs are linearly ordered, so reachability
            // from `latest_finalized_mb` via parents is iff "finalized locally".
            let latest_finalized_mb = self.db.globals().latest_finalized_mb_hash;
            if !utils::is_finalized_locally(&self.db, head_mb, latest_finalized_mb) {
                let head_meta = self.db.mb_meta(head_mb);
                tracing::warn!(
                    %head_mb,
                    %latest_finalized_mb,
                    head_computed = head_meta.computed,
                    "manager: rejecting batch — head_mb not yet finalized locally",
                );
                return Ok(ValidationStatus::Rejected {
                    request,
                    reason: ValidationRejectReason::HeadMbNotFinalized(head_mb),
                });
            }

            let head_meta = self.db.mb_meta(head_mb);
            if !head_meta.computed {
                tracing::warn!(
                    %head_mb,
                    "manager: rejecting batch — head_mb not yet computed locally",
                );
                return Ok(ValidationStatus::Rejected {
                    request,
                    reason: ValidationRejectReason::HeadMbNotComputed(head_mb),
                });
            }

            let last_committed_mb = self
                .db
                .block_meta(block.hash)
                .last_committed_mb
                .unwrap_or(H256::zero());

            // Head must strictly advance past last-committed; genesis = height 0.
            let head_height = self
                .db
                .mb_compact_block(head_mb)
                .map(|c| c.height)
                .ok_or_else(|| anyhow!("MB {head_mb} marked finalized but has no compact block"))?;
            let last_committed_height = if last_committed_mb.is_zero() {
                0
            } else {
                self.db
                    .mb_compact_block(last_committed_mb)
                    .map(|c| c.height)
                    .ok_or_else(|| {
                        anyhow!(
                            "last_committed_mb {last_committed_mb} not in DB for block {}",
                            block.hash,
                        )
                    })?
            };
            if head_height <= last_committed_height {
                tracing::warn!(
                    %head_mb,
                    head_height,
                    %last_committed_mb,
                    last_committed_height,
                    "manager: rejecting batch — head_mb at or below last_committed_mb height",
                );
                return Ok(ValidationStatus::Rejected {
                    request,
                    reason: ValidationRejectReason::HeadMbAlreadyCommitted(head_mb),
                });
            }

            // Both endpoints finalized → walk is on canonical chain; only DB-corrupt errors here.
            let pending = super::utils::collect_not_committed_mb_predecessors(
                &self.db,
                last_committed_mb,
                head_mb,
            )?;

            let mut chain_commitment = ChainCommitment {
                transitions: Vec::new(),
                head: head_mb,
                last_advanced_eth_block: self.db.mb_meta(head_mb).last_advanced_eb,
            };
            for mb_hash in pending.into_iter() {
                let Some(mb_transitions) = self.db.mb_outcome(mb_hash) else {
                    anyhow::bail!("Computed MB {mb_hash} outcome not found in db");
                };
                chain_commitment.transitions.extend(mb_transitions);
            }
            chain_commitment.transitions = super::utils::squash_transitions_by_actor(
                std::mem::take(&mut chain_commitment.transitions),
            );
            super::utils::sort_transitions_by_value_to_receive(&mut chain_commitment.transitions);
            batch_parts.chain_commitment = Some(chain_commitment);
        }

        let Some(batch) = super::utils::create_batch_commitment(
            &self.db,
            &block,
            batch_parts,
            self.limits.commitment_delay_limit,
        )?
        else {
            tracing::warn!(
                "Batch commitment is empty for block({:?}), rejecting batch",
                block.hash
            );
            return Ok(ValidationStatus::Rejected {
                request,
                reason: ValidationRejectReason::EmptyBatch,
            });
        };

        let batch_digest = batch.to_digest();
        if batch_digest != digest {
            return Ok(ValidationStatus::Rejected {
                request,
                reason: ValidationRejectReason::BatchDigestMismatch {
                    expected: digest,
                    found: batch_digest,
                },
            });
        }

        let batch_encoded_size = Gear::BatchCommitment::from(batch).abi_encoded_size() as u64;
        if batch_encoded_size > self.limits.batch_size_limit {
            return Ok(ValidationStatus::Rejected {
                request,
                reason: ValidationRejectReason::BatchSizeLimitExceeded,
            });
        }

        Ok(ValidationStatus::Accepted(digest))
    }

    pub async fn aggregate_validators_commitment(
        &self,
        block: &SimpleBlockData,
    ) -> Result<Option<ValidatorsCommitment>> {
        let (timelines, max_validators) = {
            let config = self.db.config();
            (config.timelines, config.max_validators)
        };

        let block_era = timelines
            .era_from_ts(block.header.timestamp)
            .context("failed to calculate era from block timestamp")?;
        let election_ts = timelines
            .era_election_start_ts(block_era)
            .context("failed to calculate election start timestamp")?;

        if block.header.timestamp < election_ts {
            tracing::trace!(
                block = %block.hash,
                timestamp = %block.header.timestamp,
                election_ts = %election_ts,
                genesis_ts = %timelines.genesis_ts,
                "Election period for next era has not started yet. Skipping validators commitment");

            return Ok(None);
        }

        let latest_era_validators_committed = self
            .db
            .block_meta(block.hash)
            .latest_era_validators_committed
            .ok_or_else(|| {
                anyhow!(
                    "not found latest_era_validators_committed in database for block: {}",
                    block.hash
                )
            })?;

        if latest_era_validators_committed == block_era + 1 {
            tracing::trace!(
                current_era = %block_era,
                latest_era_validators_committed = %latest_era_validators_committed,
                "Validators for next era are already committed. Skipping validators commitment"
            );

            return Ok(None);
        } else if latest_era_validators_committed > block_era + 1 {
            // This case considered as restricted,
            // because validators cannot be committed for eras later than the next one
            anyhow::bail!("validators was committed for an era later than the next one");
        } else if latest_era_validators_committed < block_era {
            tracing::warn!(
                current_era = %block_era,
                latest_era_validators_committed = %latest_era_validators_committed,
                "Validators commitment for previous eras are missing. Still try to commit validators for next era"
            );

            // TODO: !!! consider what to do if we missed commitment for previous eras,
            // currently we just try to commit for next era
        } else if latest_era_validators_committed == block_era {
            tracing::info!(
                current_era = %block_era,
                latest_era_validators_committed = %latest_era_validators_committed,
                "it is time to commit validators for next era",
            )
        } else {
            unreachable!("no other options are possible here");
        }

        let mut iter_block = *block;
        let election_block = loop {
            let parent_hash = iter_block.header.parent_hash;
            let Some(parent_header) = self.db.block_header(parent_hash) else {
                // This case can happen if node is started with fast sync and does not have full blocks history
                tracing::warn!(
                    iter_block = %iter_block.hash,
                    parent = %parent_hash,
                    "Parent block header not found when searching for election block, skipping validators commitment"
                );

                return Ok(None);
            };

            if parent_header.timestamp < election_ts {
                break iter_block;
            }

            iter_block = SimpleBlockData {
                hash: iter_block.header.parent_hash,
                header: parent_header,
            }
        };

        let request = ElectionRequest {
            at_block_hash: election_block.hash,
            at_timestamp: election_ts,
            max_validators,
        };

        let elected_validators = match self.middleware.make_election_at(request).await {
            Ok(validators) => validators,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    block = %block.hash,
                    "Failed to get elected validators from middleware, skipping validators commitment"
                );

                return Ok(None);
            }
        };

        let commitment = ValidatorsCommitment {
            has_aggregated_public_key: false,
            aggregated_public_key: Default::default(),
            verifiable_secret_sharing_commitment: Vec::new(),
            validators: elected_validators,
            era_index: block_era + 1,
        };

        Ok(Some(commitment))
    }

    // TODO #4742
    pub async fn aggregate_rewards_commitment(
        &self,
        _block: &SimpleBlockData,
    ) -> Result<Option<RewardsCommitment>> {
        Ok(None)
    }
}
