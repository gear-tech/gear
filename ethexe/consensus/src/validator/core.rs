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

use crate::utils::{self, MultisignedBatchCommitment};
use anyhow::{Result, anyhow, ensure};
use async_trait::async_trait;
use ethexe_common::{
    Address, Announce, Digest, HashOf, ProtocolTimelines, SimpleBlockData, ToDigest, ValidatorsVec,
    consensus::BatchCommitmentValidationRequest,
    db::BlockMetaStorageRO,
    ecdsa::PublicKey,
    gear::{
        BatchCommitment, ChainCommitment, CodeCommitment, RewardsCommitment, ValidatorsCommitment,
    },
};
use ethexe_db::Database;
use ethexe_ethereum::middleware::ElectionProvider;
use gprimitives::H256;
use gsigner::secp256k1::Signer;
use hashbrown::{HashMap, HashSet};
use std::{sync::Arc, time::Duration};
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

    /// Maximum deepness for chain commitment validation.
    pub validate_chain_deepness_limit: u32,
    /// Minimum deepness threshold to create chain commitment even if there are no transitions.
    pub chain_deepness_threshold: u32,
    pub block_gas_limit: u64,
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
            validate_chain_deepness_limit: self.validate_chain_deepness_limit,
            chain_deepness_threshold: self.chain_deepness_threshold,
            block_gas_limit: self.block_gas_limit,
        }
    }
}

impl ValidatorCore {
    pub async fn aggregate_batch_commitment(
        mut self,
        block: SimpleBlockData,
        announce_hash: HashOf<Announce>,
    ) -> Result<Option<BatchCommitment>> {
        let chain_commitment = self.aggregate_chain_commitment(announce_hash)?;
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
        )
    }

    pub fn aggregate_chain_commitment(
        &self,
        announce_hash: HashOf<Announce>,
    ) -> Result<Option<ChainCommitment>> {
        let Some((commitment, deepness)) =
            // Max deepness is ignored here, because we want to create chain commitment (not validate)
            utils::aggregate_chain_commitment(&self.db, announce_hash, false, None)?
        else {
            return Ok(None);
        };

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

        utils::aggregate_code_commitments(&self.db, queue, false)
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

        let election_block = utils::election_block_in_era(&self.db, block.clone(), election_ts)?;
        let request = ElectionRequest {
            at_block_hash: election_block.hash,
            at_timestamp: election_ts,
            // TODO(kuzmindev) #4908: max validators must be configurable
            max_validators: 10,
        };

        let mut elected_validators = self.middleware.make_election_at(request).await?;
        // Sort elected validators, because of we can not guarantee the determinism of validators order.
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
    ) -> Result<Digest> {
        let BatchCommitmentValidationRequest {
            digest,
            head,
            codes,
            validators,
            rewards,
        } = request;

        ensure!(
            !(head.is_none() && codes.is_empty() && !validators),
            "Empty batch (change when other commitments are supported)"
        );

        ensure!(
            !utils::has_duplicates(codes.as_slice()),
            "Duplicate codes in validation request"
        );

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
        ensure!(
            codes.iter().all(|code| waiting_codes.contains(code)),
            "Not all requested codes are waiting for commitment"
        );

        let chain_commitment = if let Some(head) = head {
            let local_announces = self.db.block_meta(block.hash).announces.ok_or_else(|| {
                anyhow!(
                    "Cannot get from db block announces for block {}",
                    block.hash
                )
            })?;
            assert_eq!(
                local_announces.len(),
                1,
                "There should be only one announce in the current block"
            );
            let local_announce = local_announces
                .first()
                .copied()
                .expect("Just checked, that there is one announce");

            // TODO #4791: support head != current block hash, have to check head is predecessor of current block
            ensure!(
                head == local_announce,
                "Head cannot be different from current block hash"
            );

            utils::aggregate_chain_commitment(
                &self.db,
                head,
                true,
                Some(self.validate_chain_deepness_limit),
            )?
            .map(|(commitment, _)| commitment)
        } else {
            None
        };

        let code_commitments = utils::aggregate_code_commitments(&self.db, codes, true)?;

        let validators_commitment = if validators {
            Self::aggregate_validators_commitment(&mut self, &block).await?
        } else {
            None
        };

        let rewards_commitment = if rewards {
            Self::aggregate_rewards_commitment(&mut self, &block).await?
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
        )?
        .ok_or_else(|| anyhow!("Batch commitment is empty for current block"))?;

        if batch.to_digest() != digest {
            Err(anyhow!(
                "Requested and local batch commitment digests mismatch"
            ))
        } else {
            Ok(digest)
        }
    }
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
    ///
    /// # Returns
    /// The hash of the transaction that was sent to the blockchain
    async fn commit_batch(self: Box<Self>, batch: MultisignedBatchCommitment) -> Result<H256>;
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
#[derive(Clone)]
pub struct MiddlewareWrapper {
    inner: Arc<dyn ElectionProvider + 'static>,
    cached_elections: Arc<RwLock<HashMap<ElectionRequest, ValidatorsVec>>>,
}

impl MiddlewareWrapper {
    #[allow(unused)]
    pub fn from_inner<M: ElectionProvider + 'static>(inner: M) -> Self {
        Self {
            inner: Arc::new(inner),
            cached_elections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn from_inner_arc(inner: Arc<dyn ElectionProvider + 'static>) -> Self {
        Self {
            inner,
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
