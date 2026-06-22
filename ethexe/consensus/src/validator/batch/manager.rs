// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::{
    filler::BatchFiller,
    types::{
        BatchLimits, BatchParts, ChainCommitmentRejection, ValidationRejectReason, ValidationStatus,
    },
    utils,
};
use crate::{
    utils, validator,
    validator::core::{ElectionRequest, MiddlewareWrapper},
};
use alloy::sol_types::SolValue;
use anyhow::{Context as _, Result, anyhow, bail};
use ethexe_common::{
    SimpleBlockData, ToDigest,
    consensus::{BatchCommitmentValidationRequest, MAX_BATCH_SIZE_LIMIT},
    db::{
        BlockMetaStorageRO, CodesStorageRO, ConfigStorageRO, GlobalsStorageRO, MbStorageRO,
        OnChainStorageRO,
    },
    gear::{
        BatchCommitment, ChainCommitment, CodeCommitment, RewardsCommitment, ValidatorsCommitment,
    },
};
use ethexe_db::Database;
use ethexe_ethereum::abi::Gear;
use gprimitives::H256;
use hashbrown::HashSet;
use std::{collections::VecDeque, fmt::format, mem};

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

    /// Coordinator-side batch builder.
    /// Creates batch commitment for the given Ethereum `block` used as a reference for the batch.
    /// Returns `Ok(None)` if batch commitment is not needed.
    /// Returns `Ok(Some(BatchCommitment))` if a batch commitment was successfully created.
    pub async fn create_batch_commitment(
        self,
        block: SimpleBlockData,
    ) -> Result<Option<BatchCommitment>> {
        let mut batch_filler = BatchFiller::new(self.limits.batch_size_limit);

        if let Some(validators_commitment) = self.aggregate_validators_commitment(block).await?
            && let Err(err) = batch_filler.include_validators_commitment(validators_commitment)
        {
            bail!("failed to include validators commitment into batch, err={err}")
        }

        if let Some(rewards_commitment) = self.aggregate_rewards_commitment(block).await?
            && let Err(err) = batch_filler.include_rewards_commitment(rewards_commitment)
        {
            bail!("failed to include rewards commitment into batch, err={err}")
        }

        // NOTE: chain commitment must be included before code commitments
        utils::try_include_chain_commitment(&self.db, block.hash, &mut batch_filler)?;

        utils::aggregate_code_commitments_for_block(&self.db, block.hash, &mut batch_filler)?;

        utils::create_batch_commitment(
            &self.db,
            &block,
            batch_filler.into_parts(),
            self.limits.commitment_delay_limit,
            self.limits.checkpoint_threshold,
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

        // NOTE: self.limits.batch_size_limit is used for batch creation, see `create_batch_commitment]`.
        // For validation, node allows batch to exceed local limit up to MAX_BATCH_SIZE_LIMIT.
        let mut batch_filler = BatchFiller::new(MAX_BATCH_SIZE_LIMIT);

        if validators {
            match self.aggregate_validators_commitment(block).await? {
                Some(commitment) => {
                    if let Err(_) = batch_filler.include_validators_commitment(commitment) {
                        return Ok(ValidationStatus::Rejected {
                            request,
                            reason: ValidationRejectReason::BatchSizeLimitExceeded,
                        });
                    }
                }
                None => {
                    return Ok(ValidationStatus::Rejected {
                        request,
                        reason: ValidationRejectReason::ValidatorsNotReady,
                    });
                }
            }
        }

        if rewards {
            match self.aggregate_rewards_commitment(block).await? {
                Some(commitment) => {
                    if let Err(_) = batch_filler.include_rewards_commitment(commitment) {
                        return Ok(ValidationStatus::Rejected {
                            request,
                            reason: ValidationRejectReason::BatchSizeLimitExceeded,
                        });
                    }
                }
                None => {
                    return Ok(ValidationStatus::Rejected {
                        request,
                        reason: ValidationRejectReason::RewardsNotReady,
                    });
                }
            }
        }

        if let Some(head_mb) = head {
            if let Some(reason) =
                self.validate_chain_commitment(block, head_mb, &mut batch_filler)?
            {
                return Ok(ValidationStatus::Rejected { request, reason });
            }
        }

        if let Some(reason) = self.validate_code_commitments(block, codes, &mut batch_filler)? {
            return Ok(ValidationStatus::Rejected { request, reason });
        }

        // Do not restrict coordinator to commit empty batch, even if checkpoint threshold is not reached.
        let checkpoint_threshold_for_validation = NonZeroU32::new(1).expect("1 != 0");

        let Some(batch) = utils::create_batch_commitment(
            &self.db,
            &block,
            batch_filler.into_parts(),
            self.limits.commitment_delay_limit,
            checkpoint_threshold_for_validation,
        )?
        else {
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

        Ok(ValidationStatus::Accepted(digest))
    }

    fn validate_chain_commitment(
        &self,
        block: SimpleBlockData,
        head_mb_hash: H256,
        batch_filler: &mut BatchFiller,
    ) -> Result<Option<ValidationRejectReason>> {
        let head_mb_meta = self.db.mb_meta(head_mb_hash);

        if !head_mb_meta.finalized {
            return Ok(Some(ValidationRejectReason::HeadMbNotFinalized(
                head_mb_hash,
            )));
        }

        if !head_mb_meta.computed {
            return Ok(Some(ValidationRejectReason::HeadMbNotComputed(
                head_mb_hash,
            )));
        }

        let head_mb = self
            .db
            .mb_compact_block(head_mb_hash)
            .with_context(|| format!("finalized MB {head_mb_hash} has no compact block in db"))?;

        let last_committed_mb_hash = self
            .db
            .block_meta(block.hash)
            .last_committed_mb
            .with_context(|| {
                format!(
                    "prepared block {} has no last_committed_mb in db",
                    block.hash
                )
            })?;
        let last_committed_mb = self
            .db
            .mb_compact_block(last_committed_mb_hash)
            .with_context(|| {
                format!("committed MB {last_committed_mb_hash} has no compact block in db")
            })?;

        // check that head_mb is a strict descendant of last_committed_mb
        let mut cursor_mb_hash = head_mb_hash;
        let mut cursor_mb = head_mb;
        let mut not_committed_mbs_chain = VecDeque::new();
        while cursor_mb.height > last_committed_mb.height {
            // push_front to keep the order from oldest to newest
            not_committed_mbs_chain.push_front(cursor_mb_hash);
            cursor_mb_hash = cursor_mb.parent;
            cursor_mb = self.db.mb_compact_block(cursor_mb_hash).with_context(|| {
                format!("finalized MB {cursor_mb_hash} has no compact block in db")
            })?;
        }

        if cursor_mb_hash != last_committed_mb_hash {
            return Ok(Some(
                ValidationRejectReason::HeadMbNotStrictDescendantOfLatestCommittedMb {
                    head_mb: head_mb_hash,
                    latest_committed_mb: last_committed_mb_hash,
                },
            ));
        }

        let last_advanced_eth_block = self
            .db
            .mb_meta(head_mb_hash)
            .last_advanced_eb
            .with_context(|| {
                format!("finalized MB {head_mb_hash} has no last_advanced_eb in db")
            })?;

        let last_committed_advanced_eth_block = self
            .db
            .mb_meta(last_committed_mb_hash)
            .last_advanced_eb
            .with_context(|| {
                format!("committed MB {last_committed_mb_hash} has no last_advanced_eb in db")
            })?;

        // This check is not necessary, as soon as this must be guaranteed by ethexe-malachite,
        // but we still want to have it just in case, to avoid accepting invalid batch commitments.
        if !utils::is_strict_descendant_eth_block(
            db,
            last_advanced_eth_block,
            last_committed_advanced_eth_block,
        ) {
            tracing::error!(
                %block,
                %head_mb_hash,
                %last_committed_mb_hash,
                %last_advanced_eth_block,
                %last_committed_advanced_eth_block,
                "head MB is finalized, but its last advanced EB is not a strict descendant of the last committed advanced EB"
            );

            return Ok(Some(
                ValidationRejectReason::LastAdvancedEbNotOnCanonicalChain {
                    last_advanced_eb: last_advanced_eth_block,
                    last_committed_advanced_eb: last_committed_advanced_eth_block,
                },
            ));
        }

        for mb_hash in not_committed_mbs_chain.into_iter() {
            let Some(transitions) = self.db.mb_outcome(mb_hash) else {
                anyhow::bail!("Computed MB {mb_hash} outcome not found in db");
            };

            let last_advanced_eth_block =
                self.db.mb_meta(mb_hash).last_advanced_eb.with_context(|| {
                    format!("finalized MB {mb_hash} has no last_advanced_eb in db")
                })?;

            let one_mb_commitment = ChainCommitment {
                head: mb_hash,
                transitions,
                last_advanced_eth_block,
            };

            if let Err(_) = batch_filler.append_chain_commitment(one_mb_commitment) {
                return Ok(Some(ValidationRejectReason::BatchSizeLimitExceeded));
            }
        }

        Ok(None)
    }

    fn validate_code_commitments(
        &self,
        block: SimpleBlockData,
        codes: &[CodeId],
        batch_filler: &mut BatchFiller,
    ) -> Result<Option<ValidationRejectReason>> {
        if utils::has_duplicates(codes.as_slice()) {
            return Ok(Some(ValidationRejectReason::CodesHaveDuplicates));
        }

        let waiting_codes = self
            .db
            .block_meta(block.hash)
            .codes_queue
            .ok_or_else(|| anyhow!("codes queue not found for block={}", block.hash))?
            .into_iter()
            .collect::<HashSet<_>>();

        if let Some(&code_id) = codes.iter().find(|&id| !waiting_codes.contains(id)) {
            return Ok(Some(ValidationRejectReason::CodeNotWaitingForCommitment(
                code_id,
            )));
        }

        for &id in codes.iter() {
            let Some(valid) = self.db.code_valid(id) else {
                return Ok(Some(ValidationRejectReason::CodeIsNotProcessedYet(id)));
            };
            let code_commitment = CodeCommitment { id, valid };
            if let Err(_) = batch_filler.include_code_commitment(code_commitment) {
                return Ok(Some(ValidationRejectReason::BatchSizeLimitExceeded));
            }
        }

        Ok(None)
    }

    async fn aggregate_validators_commitment(
        &self,
        block: SimpleBlockData,
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

        let mut cursor = block;
        let election_block = loop {
            let parent_hash = cursor.header.parent_hash;
            let Some(parent_header) = self.db.block_header(parent_hash) else {
                // This case can happen if node is started with fast sync and does not have full blocks history
                tracing::warn!(
                    iter_block = %cursor.hash,
                    parent = %parent_hash,
                    "Parent block header not found when searching for election block, skipping validators commitment"
                );

                return Ok(None);
            };

            if parent_header.timestamp < election_ts {
                break cursor;
            }

            cursor = SimpleBlockData {
                hash: cursor.header.parent_hash,
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
    async fn aggregate_rewards_commitment(
        &self,
        _block: SimpleBlockData,
    ) -> Result<Option<RewardsCommitment>> {
        Ok(None)
    }
}
