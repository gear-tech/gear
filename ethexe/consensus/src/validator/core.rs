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

use crate::validator::{ValidatorMetrics, batch::BatchCommitmentManager, tx_pool::InjectedTxPool};
use anyhow::Result;
use async_trait::async_trait;
use ethexe_common::{
    Address, ProtocolTimelines, ValidatorsVec,
    ecdsa::{ContractSignature, PublicKey},
    gear::BatchCommitment,
    injected::SignedInjectedTransaction,
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
    pub injected_pool: InjectedTxPool,
    #[debug(skip)]
    pub batch_manager: BatchCommitmentManager,
    #[debug(skip)]
    pub metrics: ValidatorMetrics,

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
            signatures_threshold: self.signatures_threshold,
            router_address: self.router_address,
            pub_key: self.pub_key,
            timelines: self.timelines,
            signer: self.signer.clone(),
            db: self.db.clone(),
            committer: self.committer.clone_boxed(),
            batch_manager: self.batch_manager.clone(),
            injected_pool: self.injected_pool.clone(),
            metrics: self.metrics.clone(),
            chain_deepness_threshold: self.chain_deepness_threshold,
            block_gas_limit: self.block_gas_limit,
            commitment_delay_limit: self.commitment_delay_limit,
            producer_delay: self.producer_delay,
        }
    }
}

impl ValidatorCore {
    pub fn process_injected_transaction(&mut self, tx: SignedInjectedTransaction) -> Result<()> {
        tracing::trace!(tx = ?tx, "Receive new injected transaction");
        self.injected_pool.handle_tx(tx);
        Ok(())
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
