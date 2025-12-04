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

//! Validator core utils and parameters.

use crate::{
    announces,
    utils::{self, CodeNotValidatedError},
    validator::tx_pool::InjectedTxPool,
};
use anyhow::{Context as _, Result, anyhow};
use async_trait::async_trait;
use ethexe_common::{
    Address, Announce, Digest, HashOf, ProtocolTimelines, SimpleBlockData, ToDigest, ValidatorsVec,
    consensus::BatchCommitmentValidationRequest,
    db::{AnnounceStorageRO, BlockMetaStorageRO, OnChainStorageRO},
    ecdsa::{ContractSignature, PublicKey},
    gear::{
        BatchCommitment, ChainCommitment, CodeCommitment, RewardsCommitment, ValidatorsCommitment,
    },
    injected::SignedInjectedTransaction,
};
use ethexe_db::Database;
use ethexe_ethereum::{middleware::ElectionProvider, router::Router};
use ethexe_signer::Signer;
use gprimitives::{CodeId, H256};
use hashbrown::{HashMap, HashSet};
use std::{hash::Hash, sync::Arc, time::Duration};
use tokio::sync::RwLock;

#[derive(derive_more::Debug)]
pub struct ValidatorCore {
    pub slot_duration: Duration,
    pub signatures_threshold: u64,
    pub router_address: Address,
    pub pub_key: PublicKey,
    pub timelines: ProtocolTimelines,

    #[debug(skip)]
    pub signer: Signer,
    #[debug(skip)]
    pub db: Database,
    #[debug(skip)]
    pub committer: Box<dyn BatchCommitter>,
    #[debug(skip)]
    pub middleware: MiddlewareWrapper,
    #[debug(skip)]
    pub injected_pool: InjectedTxPool,

    /// Maximum deepness for chain commitment validation.
    pub validate_chain_deepness_limit: u32,
    /// Minimum deepness threshold to create chain commitment even if there are no transitions.
    pub chain_deepness_threshold: u32,
    /// Gas limit to be used when creating new announce.
    pub block_gas_limit: u64,
    /// Time limit in blocks for announce to be committed after its creation.
    pub commitment_delay_limit: u32,
    /// Delay before producer starts to creating new announce after block prepared.
    pub producer_delay: Duration,
}

impl Clone for ValidatorCore {
    fn clone(&self) -> Self {
        Self {
            slot_duration: self.slot_duration,
            signatures_threshold: self.signatures_threshold,
            router_address: self.router_address,
            pub_key: self.pub_key,
            timelines: self.timelines,
            signer: self.signer.clone(),
            db: self.db.clone(),
            committer: self.committer.clone_boxed(),
            middleware: self.middleware.clone(),
            injected_pool: self.injected_pool.clone(),
            validate_chain_deepness_limit: self.validate_chain_deepness_limit,
            chain_deepness_threshold: self.chain_deepness_threshold,
            block_gas_limit: self.block_gas_limit,
            commitment_delay_limit: self.commitment_delay_limit,
            producer_delay: self.producer_delay,
        }
    }
}

impl ValidatorCore {
    pub async fn aggregate_batch_commitment(
        mut self,
        block: SimpleBlockData,
        announce_hash: HashOf<Announce>,
    ) -> Result<Option<BatchCommitment>> {
        let chain_commitment = self.aggregate_chain_commitment(block.hash, announce_hash)?;
        let code_commitments = self.aggregate_code_commitments(block.hash)?;
        let validators_commitment = self.aggregate_validators_commitment(&block).await?;
        let rewards_commitment = self.aggregate_rewards_commitment(&block).await?;

        utils::create_batch_commitment(
            &self.db,
            &block,
            chain_commitment,
            code_commitments,
            validators_commitment,
            rewards_commitment,
            self.commitment_delay_limit,
        )
    }

    pub fn aggregate_chain_commitment(
        &self,
        at_block_hash: H256,
        announce_hash: HashOf<Announce>,
    ) -> Result<Option<ChainCommitment>> {
        let (commitment, deepness) =
            utils::try_aggregate_chain_commitment(&self.db, at_block_hash, announce_hash).map_err(
                |e| anyhow!("Aggregating chain commitment for block {at_block_hash}: {e}"),
            )?;

        if commitment.transitions.is_empty() && deepness <= self.chain_deepness_threshold {
            // No transitions and chain is not deep enough, skip chain commitment
            Ok(None)
        } else {
            Ok(Some(commitment))
        }
    }

    pub fn aggregate_code_commitments(&self, block_hash: H256) -> Result<Vec<CodeCommitment>> {
        let queue =
            self.db.block_meta(block_hash).codes_queue.ok_or_else(|| {
                anyhow!("Computed block {block_hash} codes queue is not in storage")
            })?;

        Ok(utils::aggregate_code_commitments(&self.db, queue, false)
            .expect("Error is not possible here, because fail_if_not_found is false"))
    }

    pub async fn aggregate_validators_commitment(
        &mut self,
        block: &SimpleBlockData,
    ) -> Result<Option<ValidatorsCommitment>> {
        let SimpleBlockData { hash, header } = block;

        let block_era = self.timelines.era_from_ts(header.timestamp);
        let end_of_era = self.timelines.era_end(block_era);
        let election_ts = end_of_era - self.timelines.election;

        if header.timestamp < election_ts {
            tracing::trace!(
                block = %hash,
                block.timestamp = %header.timestamp,
                election_ts = %election_ts,
                end_of_era = %end_of_era,
                genesis_ts = %self.timelines.genesis_ts,
                "No election in this block, election not reached yet");
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
            tracing::debug!(
                current_era = %block_era,
                latest_era_validators_committed = ?latest_era_validators_committed,
                "Validators for next era are already committed. Skipping validators commitment"
            );
            return Ok(None);
        }

        let election_block = utils::election_block_in_era(&self.db, *block, election_ts)?;
        let request = ElectionRequest {
            at_block_hash: election_block.hash,
            at_timestamp: election_ts,
            // TODO(kuzmindev) #4908: max validators must be configurable
            max_validators: 10,
        };

        let mut elected_validators = self.middleware.make_election_at(request).await?;
        // Sort elected validators, because of RPC can not guarantee the determinism of returned validators order.
        elected_validators.sort();

        let commitment = utils::validators_commitment(block_era + 1, elected_validators)?;
        Ok(Some(commitment))
    }

    // TODO #4742
    pub async fn aggregate_rewards_commitment(
        &mut self,
        _block: &SimpleBlockData,
    ) -> Result<Option<RewardsCommitment>> {
        Ok(None)
    }

    pub async fn validate_batch_commitment_request(
        mut self,
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

        if head.is_none() && codes.is_empty() && !validators && !rewards {
            return Ok(ValidationStatus::Rejected {
                request,
                reason: ValidationRejectReason::EmptyBatch,
            });
        }

        if utils::has_duplicates(codes.as_slice()) {
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

        let chain_commitment = if let Some(head) = head {
            // TODO #4791: support commitment head from another block in chain,
            // have to check head block is predecessor of current block

            let candidates = self
                .db
                .block_meta(block.hash)
                .announces
                .into_iter()
                .flatten();
            let best_announce_hash =
                announces::best_announce(&self.db, candidates, self.commitment_delay_limit)?;
            if head != best_announce_hash {
                return Ok(ValidationStatus::Rejected {
                    request,
                    reason: ValidationRejectReason::HeadAnnounceIsNotBest {
                        requested: head,
                        best: best_announce_hash,
                    },
                });
            }

            // Head announce in validation request is best for `block`.
            // This guarantees that announce is successor of last committed announce at `block`,
            // but does not guarantee that announce is computed by this node.
            if !self.db.announce_meta(head).computed {
                return Ok(ValidationStatus::Rejected {
                    request,
                    reason: ValidationRejectReason::HeadAnnounceNotComputed(head),
                });
            }

            let (commitment, _) = utils::try_aggregate_chain_commitment(&self.db, block.hash, head)
                .context("batch commitment request validation")?;

            Some(commitment)
        } else {
            None
        };

        let code_commitments =
            match utils::aggregate_code_commitments(&self.db, codes.iter().copied(), true) {
                Ok(commitments) => commitments,
                Err(CodeNotValidatedError(code_id)) => {
                    return Ok(ValidationStatus::Rejected {
                        request,
                        reason: ValidationRejectReason::CodeIsNotProcessedYet(code_id),
                    });
                }
            };

        let validators_commitment = if validators {
            let Some(commitment) = Self::aggregate_validators_commitment(&mut self, &block).await?
            else {
                return Ok(ValidationStatus::Rejected {
                    request,
                    reason: ValidationRejectReason::ValidatorsNotReady,
                });
            };
            Some(commitment)
        } else {
            None
        };

        let rewards_commitment = if rewards {
            let Some(commitment) = Self::aggregate_rewards_commitment(&mut self, &block).await?
            else {
                return Ok(ValidationStatus::Rejected {
                    request,
                    reason: ValidationRejectReason::RewardsNotReady,
                });
            };
            Some(commitment)
        } else {
            None
        };

        let batch = utils::create_batch_commitment(
            &self.db,
            &block,
            chain_commitment,
            code_commitments,
            validators_commitment,
            rewards_commitment,
            self.commitment_delay_limit,
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

    pub fn process_injected_transaction(&mut self, tx: SignedInjectedTransaction) -> Result<()> {
        tracing::trace!(tx = ?tx, "Receive new injected transaction");
        self.injected_pool.handle_tx(tx);
        Ok(())
    }
}

// pub enum ValidatorsCommitmentStatus {
//     Completed(ValidatorsCommitment),

// }

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
}

/// Trait for committing batch commitments to the blockchain.
#[async_trait]
pub trait BatchCommitter: Send {
    /// Creates a boxed clone of the committer.
    fn clone_boxed(&self) -> Box<dyn BatchCommitter>;

    /// Commits a batch of signed commitments to the blockchain.
    ///
    /// # Arguments
    /// * `batch` - The batch of commitments to commit
    /// * `signatures` - The signatures for the batch commitments
    ///
    /// # Returns
    /// The hash of the transaction that was sent to the blockchain
    async fn commit(
        self: Box<Self>,
        batch: BatchCommitment,
        signatures: Vec<ContractSignature>,
    ) -> Result<H256>;
}

impl<T: BatchCommitter + 'static> From<T> for Box<dyn BatchCommitter> {
    fn from(committer: T) -> Self {
        Box::new(committer)
    }
}

/// [`ElectionRequest`] determines the moment when validators election happen.
/// If requests are equal result can be reused by [`MiddlewareWrapper`] to reduce the amount of rpc calls.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ElectionRequest {
    at_block_hash: H256,
    at_timestamp: u64,
    max_validators: u32,
}

/// [`MiddlewareWrapper`] is a wrapper around the dyn [`ElectionProvider`] trait.
/// It caches the elections results to reduce the number of rpc calls.
pub struct MiddlewareWrapper {
    inner: Box<dyn ElectionProvider>,
    cached_elections: Arc<RwLock<HashMap<ElectionRequest, ValidatorsVec>>>,
}

impl Clone for MiddlewareWrapper {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone_boxed(),
            cached_elections: self.cached_elections.clone(),
        }
    }
}

impl MiddlewareWrapper {
    pub fn from_inner(inner: impl Into<Box<dyn ElectionProvider>>) -> Self {
        Self {
            inner: inner.into(),
            cached_elections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn make_election_at(&self, request: ElectionRequest) -> Result<ValidatorsVec> {
        if let Some(cached_result) = self.cached_elections.read().await.get(&request) {
            return Ok(cached_result.clone());
        }

        let elected_validators = self
            .inner
            .make_election_at(request.at_timestamp, request.max_validators as u128)
            .await?;

        self.cached_elections
            .write()
            .await
            .insert(request, elected_validators.clone());

        Ok(elected_validators)
    }
}

#[async_trait]
impl BatchCommitter for Router {
    fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
        Box::new(self.clone())
    }

    async fn commit(
        self: Box<Self>,
        batch: BatchCommitment,
        signatures: Vec<ContractSignature>,
    ) -> Result<H256> {
        tracing::debug!("Batch commitment to submit: {batch:?}");

        self.commit_batch(batch, signatures).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::*, validator::mock::*};
    use ethexe_common::mock::*;
    use gear_core::ids::prelude::CodeIdExt;

    fn unwrap_rejected_reason(status: ValidationStatus) -> ValidationRejectReason {
        match status {
            ValidationStatus::Rejected { reason, .. } => reason,
            ValidationStatus::Accepted(digest) => {
                panic!(
                    "Expected rejection, but got acceptance with digest {:?}",
                    digest
                )
            }
        }
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn rejects_empty_batch_request() {
        gear_utils::init_default_logger();

        let (ctx, _, _) = mock_validator_context();
        let empty_request = BatchCommitmentValidationRequest {
            digest: Digest::zero(),
            head: None,
            codes: vec![],
            validators: false,
            rewards: false,
        };

        let status = ctx
            .core
            .validate_batch_commitment_request(SimpleBlockData::mock(()), empty_request)
            .await
            .unwrap();

        assert_eq!(
            unwrap_rejected_reason(status),
            ValidationRejectReason::EmptyBatch
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn rejects_duplicate_code_ids() {
        gear_utils::init_default_logger();

        let (ctx, _, _) = mock_validator_context();
        let mut batch = prepare_chain_for_batch_commitment(&ctx.core.db);
        let duplicate = batch.code_commitments[0].clone();
        batch.code_commitments.push(duplicate);

        let status = ctx
            .core
            .validate_batch_commitment_request(
                SimpleBlockData::mock(()),
                BatchCommitmentValidationRequest::new(&batch),
            )
            .await
            .unwrap();

        assert_eq!(
            unwrap_rejected_reason(status),
            ValidationRejectReason::CodesHasDuplicates
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn rejects_not_waiting_code_ids() {
        gear_utils::init_default_logger();

        let (ctx, _, _) = mock_validator_context();
        let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
        let block = ctx.core.db.simple_block_data(batch.block_hash);
        let mut request = BatchCommitmentValidationRequest::new(&batch);

        let missing_code = H256::random().into();
        request.codes.push(missing_code);

        let status = ctx
            .core
            .validate_batch_commitment_request(block, request)
            .await
            .unwrap();

        assert_eq!(
            unwrap_rejected_reason(status),
            ValidationRejectReason::CodeNotWaitingForCommitment(missing_code)
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn rejects_non_best_chain_head() {
        gear_utils::init_default_logger();

        let (ctx, _, _) = mock_validator_context();
        let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
        let block = ctx.core.db.simple_block_data(batch.block_hash);
        let mut request = BatchCommitmentValidationRequest::new(&batch);
        let best_head = request.head.expect("chain commitment expected");

        let wrong_head = HashOf::random();
        request.head = Some(wrong_head);

        let status = ctx
            .core
            .validate_batch_commitment_request(block, request)
            .await
            .unwrap();

        assert_eq!(
            unwrap_rejected_reason(status),
            ValidationRejectReason::HeadAnnounceIsNotBest {
                requested: wrong_head,
                best: best_head,
            }
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn rejects_digest_mismatch() {
        gear_utils::init_default_logger();

        let (ctx, _, _) = mock_validator_context();
        let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
        let block = ctx.core.db.simple_block_data(batch.block_hash);
        let mut request = BatchCommitmentValidationRequest::new(&batch);
        let original_digest = request.digest;
        let mut wrong_digest = original_digest;
        while wrong_digest == original_digest {
            wrong_digest = Digest::random();
        }
        request.digest = wrong_digest;

        let status = ctx
            .core
            .validate_batch_commitment_request(block, request)
            .await
            .unwrap();

        assert_eq!(
            unwrap_rejected_reason(status),
            ValidationRejectReason::BatchDigestMismatch {
                expected: wrong_digest,
                found: original_digest,
            }
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn rejects_code_not_processed_yet() {
        gear_utils::init_default_logger();

        let (ctx, _, _) = mock_validator_context();
        let code = b"1234";
        let code_id = CodeId::generate(code);
        let chain = BlockChain::mock(10)
            .tap_mut(|chain| {
                chain.blocks[10]
                    .as_prepared_mut()
                    .codes_queue
                    .push_front(code_id);
                chain.codes.insert(
                    code_id,
                    CodeData {
                        original_bytes: code.to_vec(),
                        blob_info: Default::default(),
                        instrumented: None,
                    },
                );
            })
            .setup(&ctx.core.db);
        let block = chain.blocks[10].to_simple();
        let code_commitments = vec![CodeCommitment {
            id: code_id,
            valid: true,
        }];
        let batch = utils::create_batch_commitment(
            &ctx.core.db,
            &block,
            None,
            code_commitments,
            None,
            None,
            100,
        )
        .unwrap()
        .unwrap();

        let status = ctx
            .core
            .validate_batch_commitment_request(block, BatchCommitmentValidationRequest::new(&batch))
            .await
            .unwrap();

        assert_eq!(
            unwrap_rejected_reason(status),
            ValidationRejectReason::CodeIsNotProcessedYet(code_id)
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn accepts_matching_request() {
        gear_utils::init_default_logger();

        let (ctx, _, _) = mock_validator_context();
        let batch = prepare_chain_for_batch_commitment(&ctx.core.db);
        let block = ctx.core.db.simple_block_data(batch.block_hash);
        let request = BatchCommitmentValidationRequest::new(&batch);
        let expected_digest = request.digest;

        let status = ctx
            .core
            .validate_batch_commitment_request(block, request)
            .await
            .unwrap();

        match status {
            ValidationStatus::Accepted(digest) => assert_eq!(digest, expected_digest),
            ValidationStatus::Rejected { reason, .. } => {
                panic!("Expected acceptance, got rejection: {reason:?}")
            }
        }
    }
}
