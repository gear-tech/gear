// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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
        batch::{filler::BatchFiller, utils},
        core::{ElectionRequest, MiddlewareWrapper},
    },
};

use anyhow::{Result, anyhow, bail};
use ethexe_common::{
    Announce, HashOf, ProtocolTimelines, SimpleBlockData, ToDigest,
    consensus::BatchCommitmentValidationRequest,
    db::{AnnounceStorageRO, BlockMetaStorageRO, OnChainStorageRO},
    gear::{
        BatchCommitment, ChainCommitment, CodeCommitment, RewardsCommitment, ValidatorsCommitment,
    },
};
use ethexe_db::Database;
use gprimitives::H256;
use hashbrown::HashSet;

#[derive(derive_more::Debug, Clone)]
pub struct BatchCommitmentManager {
    /// Limits for batch building and verifying
    limits: BatchLimits,
    // TODO: hack for tests, remove this `pub(crate)`
    pub(crate) timelines: ProtocolTimelines,
    #[debug(skip)]
    db: Database,
    #[debug(skip)]
    middleware: MiddlewareWrapper,
}

impl BatchCommitmentManager {
    pub fn new(
        limits: BatchLimits,
        timelines: ProtocolTimelines,
        db: Database,
        middleware: MiddlewareWrapper,
    ) -> Self {
        Self {
            limits,
            timelines,
            db,
            middleware,
        }
    }

    /// Maybe rename this function
    pub async fn build(
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

        let not_committed_announces =
            super::utils::collect_not_committed_predecessors(&self.db, block.hash, announce_hash)?;

        for (deep, announce_hash) in not_committed_announces.into_iter().enumerate() {
            let transitions = super::utils::announce_transitions(&self.db, announce_hash)?;
            let chain_commitment = ChainCommitment {
                head_announce: announce_hash,
                transitions,
            };

            let announce_block_hash = self
                .db
                .announce(announce_hash)
                .ok_or_else(|| anyhow!(""))?
                .block_hash;

            let commitments = self.aggregate_code_commitments(announce_block_hash)?;

            if let Err(err) = batch_filler.include_chain_and_codes_commitments(
                chain_commitment,
                deep as u32,
                commitments,
            ) {
                tracing::trace!(
                    "failed to include transitions and codes in batch for announce({announce_hash}) because of error: {err}"
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

    pub async fn validate(
        self,
        block: SimpleBlockData,
        request: BatchCommitmentValidationRequest,
    ) -> Result<ValidationStatus> {
        if request.is_empty() {
            return Ok(ValidationStatus::Rejected {
                request,
                reason: ValidationRejectReason::EmptyBatch,
            });
        }

        let &BatchCommitmentValidationRequest {
            digest,
            head,
            ref codes,
            validators,
            rewards,
        } = &request;

        let mut batch_filler = BatchFiller::new(self.limits.clone());

        if crate::utils::has_duplicates(codes.as_slice()) {
            return Ok(ValidationStatus::Rejected {
                request,
                reason: ValidationRejectReason::CodesHasDuplicates,
            });
        }

        // Check requested codes wait for commitment
        let waiting_codes = self
            .db
            .block_meta(block.hash)
            .codes_queue
            .ok_or_else(|| {
                anyhow!(
                    "Cannot get from db block codes queue for block {}",
                    block.hash
                )
            })?
            .into_iter()
            .collect::<HashSet<_>>();
        if let Some(&code_id) = codes.iter().find(|&id| !waiting_codes.contains(id)) {
            return Ok(ValidationStatus::Rejected {
                request,
                reason: ValidationRejectReason::CodeNotWaitingForCommitment(code_id),
            });
        }
        let code_commitments =
            match super::utils::aggregate_code_commitments(&self.db, codes.iter().copied(), true) {
                Ok(commitments) => commitments,
                Err(CodeNotValidatedError(code_id)) => {
                    return Ok(ValidationStatus::Rejected {
                        request,
                        reason: ValidationRejectReason::CodeIsNotProcessedYet(code_id),
                    });
                }
            };
        if let Err(err) = batch_filler.include_code_commitments(code_commitments) {
            let reason = err.into();
            return Ok(ValidationStatus::Rejected { request, reason });
        }

        if let Some(head) = head {
            // TODO #4791: support commitment head from another block in chain,
            // have to check head block is predecessor of current block

            // Head announce in validation request is best for `block`.
            // This guarantees that announce is successor of last committed announce at `block`,
            // but does not guarantee that announce is computed by this node.
            if !self.db.announce_meta(head).computed {
                return Ok(ValidationStatus::Rejected {
                    request,
                    reason: ValidationRejectReason::HeadAnnounceNotComputed(head),
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

            let not_committed_announces = match utils::collect_not_committed_predecessors(
                &self.db,
                block.hash,
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
            tracing::trace!("not computed announces = {not_committed_announces:?}");

            if !not_committed_announces.contains(&head) {
                // TODO: fix the rejection reason
                return Ok(ValidationStatus::Rejected {
                    request,
                    reason: ValidationRejectReason::HeadAnnounceIsNotBest {
                        requested: head,
                        best: best_announce_hash,
                    },
                });
            }

            let (chain_commitment, deepness) =
                utils::try_aggregate_chain_commitment(&self.db, block.hash, head)?;

            if let Err(err) = batch_filler.include_chain_commitment(chain_commitment, deepness) {
                let reason = err.into();
                return Ok(ValidationStatus::Rejected { request, reason });
            }

            // if !not_committed_announces.contains(&head) {
            //     // TODO: fix the rejection reason
            //     return Ok(ValidationStatus::Rejected {
            //         request,
            //         reason: ValidationRejectReason::HeadAnnounceIsNotBest {
            //             requested: head,
            //             best: best_announce_hash,
            //         },
            //     });
            // }

            // for (deep, announce_hash) in not_committed_announces.into_iter().enumerate() {
            //     let transitions = super::utils::announce_transitions(&self.db, announce_hash)?;
            //     let commitment = ChainCommitment {
            //         head_announce: announce_hash,
            //         transitions,
            //     };

            //     if let Err(err) = batch_filler.include_chain_commitment(commitment, deep) {
            //         let reason = err.into();
            //         return Ok(ValidationStatus::Rejected { request, reason });
            //     }

            // let announce_block_hash = self
            //     .db
            //     .announce(announce_hash)
            //     .ok_or_else(|| anyhow!(""))?
            //     .block_hash;

            // let commitments = self.aggregate_code_commitments(announce_block_hash)?;

            // if let Err(err) = batch_filler.include_chain_and_codes_commitments(
            //     chain_commitment,
            //     deep + 1,
            //     commitments,
            // ) {
            //     let reason = err.into();
            //     return Ok(ValidationStatus::Rejected { request, reason });
            // }

            //     if announce_hash == head {
            //         break;
            //     }
            // }
        }

        if validators {
            match self.aggregate_validators_commitment(&block).await? {
                Some(commitment) => {
                    if let Err(err) = batch_filler.include_validators_commitment(commitment) {
                        let reason = err.into();
                        return Ok(ValidationStatus::Rejected { request, reason });
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
            match self.aggregate_rewards_commitment(&block).await? {
                Some(commitment) => {
                    if let Err(err) = batch_filler.include_rewards_commitment(commitment) {
                        let reason = err.into();
                        return Ok(ValidationStatus::Rejected { request, reason });
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

        let batch = super::utils::create_batch_commitment(
            &self.db,
            &block,
            batch_filler.into_parts(),
            self.limits.commitment_delay_limit,
        )?
        .ok_or_else(|| anyhow!("Batch commitment is empty for current block"))?;

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

    pub fn aggregate_code_commitments(&self, block_hash: H256) -> Result<Vec<CodeCommitment>> {
        let queue =
            self.db.block_meta(block_hash).codes_queue.ok_or_else(|| {
                anyhow!("Computed block {block_hash} codes queue is not in storage")
            })?;

        Ok(
            super::utils::aggregate_code_commitments(&self.db, queue, false)
                .expect("Error is not possible here, because fail_if_not_found is false"),
        )
    }

    pub async fn aggregate_validators_commitment(
        &self,
        block: &SimpleBlockData,
    ) -> Result<Option<ValidatorsCommitment>> {
        let block_era = self.timelines.era_from_ts(block.header.timestamp);
        let election_ts = self.timelines.era_election_start_ts(block_era);

        if block.header.timestamp < election_ts {
            tracing::trace!(
                block = %block.hash,
                timestamp = %block.header.timestamp,
                election_ts = %election_ts,
                genesis_ts = %self.timelines.genesis_ts,
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
