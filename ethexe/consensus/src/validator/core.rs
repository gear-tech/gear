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
    BatchCommitmentValidationRequest,
    utils::{self, MultisignedBatchCommitment},
};
use anyhow::{Result, anyhow, ensure};
use async_trait::async_trait;
use ethexe_common::{
    Address, Digest, SimpleBlockData, ToDigest,
    db::BlockMetaStorageRead,
    ecdsa::PublicKey,
    gear::{
        BatchCommitment, ChainCommitment, CodeCommitment, RewardsCommitment, ValidatorsCommitment,
    },
};
use ethexe_db::Database;
use ethexe_ethereum::middleware::Middleware;
use futures::lock::Mutex;
use gprimitives::H256;
use gsigner::secp256k1::Signer;
use std::{collections::HashSet, sync::Arc, time::Duration};

#[derive(derive_more::Debug)]
pub struct ValidatorCore {
    pub slot_duration: Duration,
    pub signatures_threshold: u64,
    pub router_address: Address,
    pub pub_key: PublicKey,

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
    ) -> Result<Option<BatchCommitment>> {
        let chain_commitment = self.aggregate_chain_commitment(block.hash)?;
        let code_commitments = self.aggregate_code_commitments(block.hash)?;
        let validators_commitment = self.aggregate_validators_commitment(&block).await?;
        let rewards_commitment = self.aggregate_rewards_commitment(&block).await?;

        if chain_commitment.is_none()
            && code_commitments.is_empty()
            && validators_commitment.is_none()
            && rewards_commitment.is_none()
        {
            log::debug!(
                "No commitments for block {} - skip batch commitment",
                block.hash
            );
            return Ok(None);
        }

        utils::create_batch_commitment(
            &self.db,
            &block,
            chain_commitment,
            code_commitments,
            validators_commitment,
            rewards_commitment,
        )
    }

    pub fn aggregate_chain_commitment(&self, block_hash: H256) -> Result<Option<ChainCommitment>> {
        let head_announce = self
            .db
            .block_meta(block_hash)
            .announces
            .into_iter()
            .flat_map(|a| a.into_iter())
            .next()
            .ok_or_else(|| anyhow!("No announces found for {block_hash} in block meta storage"))?;

        let Some((commitment, deepness)) =
            // Max deepness is ignored here, because we want to create chain commitment (not validate)
            utils::aggregate_chain_commitment(&self.db, head_announce, false, None)?
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

    // TODO #4741
    pub async fn aggregate_validators_commitment(
        &mut self,
        _block: &SimpleBlockData,
    ) -> Result<Option<ValidatorsCommitment>> {
        // self.middleware.make_election_at(ElectionRequest {
        //     at_block_hash: todo!(),
        //     at_timestamp: todo!(),
        //     max_validators: todo!(),
        // });
        Ok(None)
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
            !(head.is_none() && codes.is_empty()),
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ElectionRequest {
    at_block_hash: H256,
    at_timestamp: u64,
    max_validators: u32,
}

#[async_trait]
pub trait MiddlewareExt: Send {
    /// Creates a boxed clone.
    fn clone_boxed(&self) -> Box<dyn MiddlewareExt>;

    /// Requests the election of validators at a specific block and timestamp.
    async fn make_election_at(self: Box<Self>, request: ElectionRequest) -> Result<Vec<Address>>;
}

pub struct MiddlewareWrapper {
    inner: Box<dyn MiddlewareExt>,
    db: Database,
    #[allow(clippy::type_complexity)]
    cached_election_result: Arc<Mutex<Option<(ElectionRequest, Vec<Address>)>>>,
}

impl Clone for MiddlewareWrapper {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone_boxed(),
            db: self.db.clone(),
            cached_election_result: self.cached_election_result.clone(),
        }
    }
}

impl MiddlewareWrapper {
    pub fn new(inner: Box<dyn MiddlewareExt>, db: Database) -> Self {
        Self {
            inner,
            db,
            cached_election_result: Arc::new(Mutex::new(None)),
        }
    }

    #[allow(unused)]
    pub async fn make_election_at(&self, request: ElectionRequest) -> Result<Vec<Address>> {
        let mut cached = self.cached_election_result.lock().await;

        if let Some((_cached_request, _cached_result)) = &*cached {
            // TODO: implement this. If cached_request has same at_timestamp and max_validators and
            // new request at_block_hash is a successor of cached one, then we can reuse cached.
            Ok(vec![])
        } else {
            log::debug!("Making new election request to rpc: {request:?}");

            let result = self
                .inner
                .clone_boxed()
                .make_election_at(request.clone())
                .await?;

            let result: Vec<Address> = result.into_iter().collect();

            *cached = Some((request, result.clone()));

            Ok(result)
        }
    }
}

#[async_trait]
impl MiddlewareExt for Middleware {
    fn clone_boxed(&self) -> Box<dyn MiddlewareExt> {
        Box::new(self.clone())
    }

    async fn make_election_at(self: Box<Self>, request: ElectionRequest) -> Result<Vec<Address>> {
        let ElectionRequest {
            // TODO #4741: use at_block_hash in rpc call
            at_block_hash: _,
            at_timestamp,
            max_validators,
        } = request;

        self.query()
            .make_election_at(at_timestamp, max_validators as u128)
            .await
    }
}

#[async_trait]
impl MiddlewareExt for () {
    fn clone_boxed(&self) -> Box<dyn MiddlewareExt> {
        Box::new(())
    }

    async fn make_election_at(self: Box<Self>, _request: ElectionRequest) -> Result<Vec<Address>> {
        Err(anyhow!("Middleware is not configured"))
    }
}
