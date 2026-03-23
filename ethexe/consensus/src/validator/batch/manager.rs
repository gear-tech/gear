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

use super::types::{BatchLimits, CodeNotValidatedError, ValidationRejectReason, ValidationStatus};
use crate::{
    announces,
    validator::{
        batch::{filler::BatchFiller, types::BatchParts, utils},
        core::{ElectionRequest, MiddlewareWrapper},
    },
};

use alloy::sol_types::SolValue;
use anyhow::{Result, anyhow, bail};
use ethexe_common::{
    Announce, HashOf, SimpleBlockData, ToDigest,
    consensus::{BatchCommitmentValidationRequest, MAX_BATCH_SIZE_LIMIT},
    db::{AnnounceStorageRO, BlockMetaStorageRO, ConfigStorageRO, OnChainStorageRO},
    gear::{BatchCommitment, ChainCommitment, RewardsCommitment, ValidatorsCommitment},
};
use ethexe_db::Database;
use ethexe_ethereum::abi::Gear;
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

    /// Replaces current limits with `new_limits` and returns the previous limits.
    #[cfg(test)]
    pub fn replace_limits(&mut self, new_limits: BatchLimits) -> BatchLimits {
        std::mem::replace(&mut self.limits, new_limits)
    }

    /// Creates a new [`BatchCommitment`] for producer.
    pub async fn create_batch_commitment(
        self,
        block: SimpleBlockData,
        announce_hash: HashOf<Announce>,
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

        // NOTE: we prioritize state transitions over code commitments. So include them firstly.
        super::utils::try_include_chain_commitment(
            &self.db,
            block.hash,
            announce_hash,
            &mut batch_filler,
        )?;

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

        if let Some(announce) = head {
            // Head announce in validation request is best for `block`.
            // This guarantees that announce is successor of last committed announce at `block`,
            // but does not guarantee that announce is computed by this node.
            if !self.db.announce_meta(announce).computed {
                return Ok(ValidationStatus::Rejected {
                    request,
                    reason: ValidationRejectReason::HeadAnnounceNotComputed(announce),
                });
            }

            let candidates = self
                .db
                .block_meta(block.hash)
                .announces
                .into_iter()
                .flatten();

            let best_announce_hash =
                announces::best_announce(&self.db, candidates, self.limits.commitment_delay_limit)?;

            let Some(last_committed_announce) =
                self.db.block_meta(block.hash).last_committed_announce
            else {
                anyhow::bail!(
                    "Last committed announce not found in db for prepared block: {}",
                    block.hash
                );
            };

            let not_committed_announces = match utils::collect_not_committed_predecessors(
                &self.db,
                last_committed_announce,
                best_announce_hash,
            ) {
                Ok(announces) => announces,
                Err(err) => {
                    tracing::debug!(
                        block = %block.hash,
                        best_announce = %best_announce_hash,
                        error = %err,
                        "failed to collect not committed predecessors for best announce during batch validation"
                    );
                    return Ok(ValidationStatus::Rejected {
                        request,
                        reason: ValidationRejectReason::BestHeadAnnounceChainInvalid(
                            best_announce_hash,
                        ),
                    });
                }
            };

            if !not_committed_announces.contains(&announce) {
                return Ok(ValidationStatus::Rejected {
                    request,
                    reason: ValidationRejectReason::HeadAnnounceIsNotFromBestChain {
                        requested: announce,
                        best: best_announce_hash,
                    },
                });
            }
            // Set firstly for current announce.
            let mut chain_commitment = ChainCommitment {
                transitions: Vec::new(),
                head_announce: announce,
            };
            for announce_hash in not_committed_announces.into_iter() {
                let transitions = super::utils::announce_transitions(&self.db, announce_hash)?;
                chain_commitment.transitions.extend(transitions);
                // TODO: make it clearly
                if announce_hash == announce {
                    break;
                }
            }
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
        if batch_encoded_size > MAX_BATCH_SIZE_LIMIT {
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
        let timelines = self.db.config().timelines;

        let block_era = timelines.era_from_ts(block.header.timestamp);
        let election_ts = timelines.era_election_start_ts(block_era);

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
            .block_validators_committed_for_era(block.hash)
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
            // TODO #4908: max validators must be configurable
            max_validators: 10,
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

        let (aggregated_public_key, verifiable_secret_sharing_commitment) =
            match crate::utils::generate_roast_keys(&elected_validators) {
                Ok(keys) => keys,
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        block = %block.hash,
                        "Failed to generate ROAST keys for elected validators, skipping validators commitment"
                    );

                    return Ok(None);
                }
            };

        let commitment = ValidatorsCommitment {
            aggregated_public_key,
            verifiable_secret_sharing_commitment,
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
