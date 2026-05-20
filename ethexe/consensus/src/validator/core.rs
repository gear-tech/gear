// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Validator core utils and parameters.

use crate::validator::{ValidatorMetrics, batch::BatchCommitmentManager};
use anyhow::Result;
use async_trait::async_trait;
use ethexe_common::{
    Address, ProtocolTimelines, ValidatorsVec,
    ecdsa::{ContractSignature, PublicKey},
    gear::BatchCommitment,
};
use ethexe_db::Database;
use ethexe_ethereum::{middleware::ElectionProvider, router::Router};
use gprimitives::H256;
use gsigner::secp256k1::Signer;
use hashbrown::HashMap;
use std::{hash::Hash, sync::Arc, time::Duration};
use tokio::sync::RwLock;

#[derive(derive_more::Debug)]
pub struct ValidatorCore {
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
    pub batch_manager: BatchCommitmentManager,
    #[debug(skip)]
    pub metrics: ValidatorMetrics,

    /// Coordinator-local lifetime (Eth blocks) of a fresh `BatchCommitment`
    /// past its target block — copied into
    /// [`BatchCommitment::expiry`](ethexe_common::gear::BatchCommitment).
    pub commitment_delay_limit: std::num::NonZero<u8>,
    /// Delay between receiving a new chain head and the coordinator
    /// starting batch aggregation. Buys time for participants to receive
    /// the same chain head and for malachite to finalize mb with fresh post-quarantine block included.
    /// Anyway not necessary.
    pub coordinator_aggregation_delay: Duration,
}

impl Clone for ValidatorCore {
    fn clone(&self) -> Self {
        Self {
            signatures_threshold: self.signatures_threshold,
            router_address: self.router_address,
            pub_key: self.pub_key,
            timelines: self.timelines,
            signer: self.signer.clone(),
            db: self.db.clone(),
            committer: self.committer.clone_boxed(),
            batch_manager: self.batch_manager.clone(),
            metrics: self.metrics.clone(),
            commitment_delay_limit: self.commitment_delay_limit,
            coordinator_aggregation_delay: self.coordinator_aggregation_delay,
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
    pub at_block_hash: H256,
    pub at_timestamp: u64,
    pub max_validators: u16,
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
