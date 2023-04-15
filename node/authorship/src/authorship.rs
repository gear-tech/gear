// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use codec::Encode;
use futures::{
    channel::oneshot,
    future,
    future::{Future, FutureExt},
    select,
};
use log::{debug, error, info, trace, warn};
use pallet_gear_rpc_runtime_api::GearApi as GearRuntimeApi;
use sc_block_builder::{BlockBuilderApi, BlockBuilderProvider};
use sc_client_api::backend;
use sc_telemetry::{telemetry, TelemetryHandle, CONSENSUS_INFO};
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::{ApplyExtrinsicFailed::Validity, Error::ApplyExtrinsicFailed, HeaderBackend};
use sp_consensus::{DisableProofRecording, EnableProofRecording, ProofRecording, Proposal};
use sp_core::traits::SpawnNamed;
use sp_inherents::InherentData;
use sp_runtime::{
    traits::{BlakeTwo256, Block as BlockT, Hash as HashT, Header as HeaderT},
    Digest, Percent, SaturatedConversion,
};
use std::{marker::PhantomData, pin::Pin, sync::Arc, time};

use crate::block_builder::BlockBuilderExt;
use prometheus_endpoint::Registry as PrometheusRegistry;
use sc_proposer_metrics::{EndProposingReason, MetricsLink as PrometheusMetrics};

/// Default block size limit in bytes used by [`Proposer`].
///
/// Can be overwritten by [`ProposerFactory::set_default_block_size_limit`].
///
/// Be aware that there is also an upper packet size on what the networking code
/// will accept. If the block doesn't fit in such a package, it can not be
/// transferred to other nodes.
pub const DEFAULT_BLOCK_SIZE_LIMIT: usize = 4 * 1024 * 1024 + 512;

const DEFAULT_SOFT_DEADLINE_PERCENT: Percent = Percent::from_percent(50);

/// [`Proposer`] factory.
pub struct ProposerFactory<A, B, C, PR> {
    spawn_handle: Box<dyn SpawnNamed>,
    /// The client instance.
    client: Arc<C>,
    /// The transaction pool.
    transaction_pool: Arc<A>,
    /// Prometheus Link,
    metrics: PrometheusMetrics,
    /// The default block size limit.
    ///
    /// If no `block_size_limit` is passed to [`sp_consensus::Proposer::propose`], this block size
    /// limit will be used.
    default_block_size_limit: usize,
    /// Soft deadline percentage of hard deadline.
    ///
    /// The value is used to compute soft deadline during block production.
    /// The soft deadline indicates where we should stop attempting to add transactions
    /// to the block, which exhaust resources. After soft deadline is reached,
    /// we switch to a fixed-amount mode, in which after we see `MAX_SKIPPED_TRANSACTIONS`
    /// transactions which exhaust resrouces, we will conclude that the block is full.
    soft_deadline_percent: Percent,
    telemetry: Option<TelemetryHandle>,
    /// When estimating the block size, should the proof be included?
    include_proof_in_block_size_estimation: bool,
    /// phantom member to pin the `Backend`/`ProofRecording` type.
    _phantom: PhantomData<(B, PR)>,
}

impl<A, B, C> ProposerFactory<A, B, C, DisableProofRecording> {
    /// Create a new proposer factory.
    ///
    /// Proof recording will be disabled when using proposers built by this instance to build
    /// blocks.
    pub fn new(
        spawn_handle: impl SpawnNamed + 'static,
        client: Arc<C>,
        transaction_pool: Arc<A>,
        prometheus: Option<&PrometheusRegistry>,
        telemetry: Option<TelemetryHandle>,
    ) -> Self {
        ProposerFactory {
            spawn_handle: Box::new(spawn_handle),
            transaction_pool,
            metrics: PrometheusMetrics::new(prometheus),
            default_block_size_limit: DEFAULT_BLOCK_SIZE_LIMIT,
            soft_deadline_percent: DEFAULT_SOFT_DEADLINE_PERCENT,
            telemetry,
            client,
            include_proof_in_block_size_estimation: false,
            _phantom: PhantomData,
        }
    }
}

impl<A, B, C> ProposerFactory<A, B, C, EnableProofRecording> {
    /// Create a new proposer factory with proof recording enabled.
    ///
    /// Each proposer created by this instance will record a proof while building a block.
    ///
    /// This will also include the proof into the estimation of the block size. This can be disabled
    /// by calling [`ProposerFactory::disable_proof_in_block_size_estimation`].
    pub fn with_proof_recording(
        spawn_handle: impl SpawnNamed + 'static,
        client: Arc<C>,
        transaction_pool: Arc<A>,
        prometheus: Option<&PrometheusRegistry>,
        telemetry: Option<TelemetryHandle>,
    ) -> Self {
        ProposerFactory {
            client,
            spawn_handle: Box::new(spawn_handle),
            transaction_pool,
            metrics: PrometheusMetrics::new(prometheus),
            default_block_size_limit: DEFAULT_BLOCK_SIZE_LIMIT,
            soft_deadline_percent: DEFAULT_SOFT_DEADLINE_PERCENT,
            telemetry,
            include_proof_in_block_size_estimation: true,
            _phantom: PhantomData,
        }
    }

    /// Disable the proof inclusion when estimating the block size.
    pub fn disable_proof_in_block_size_estimation(&mut self) {
        self.include_proof_in_block_size_estimation = false;
    }
}

impl<A, B, C, PR> ProposerFactory<A, B, C, PR> {
    /// Set the default block size limit in bytes.
    ///
    /// The default value for the block size limit is:
    /// [`DEFAULT_BLOCK_SIZE_LIMIT`].
    ///
    /// If there is no block size limit passed to [`sp_consensus::Proposer::propose`], this value
    /// will be used.
    pub fn set_default_block_size_limit(&mut self, limit: usize) {
        self.default_block_size_limit = limit;
    }

    /// Set soft deadline percentage.
    ///
    /// The value is used to compute soft deadline during block production.
    /// The soft deadline indicates where we should stop attempting to add transactions
    /// to the block, which exhaust resources. After soft deadline is reached,
    /// we switch to a fixed-amount mode, in which after we see `MAX_SKIPPED_TRANSACTIONS`
    /// transactions which exhaust resrouces, we will conclude that the block is full.
    ///
    /// Setting the value too low will significantly limit the amount of transactions
    /// we try in case they exhaust resources. Setting the value too high can
    /// potentially open a DoS vector, where many "exhaust resources" transactions
    /// are being tried with no success, hence block producer ends up creating an empty block.
    pub fn set_soft_deadline(&mut self, percent: Percent) {
        self.soft_deadline_percent = percent;
    }
}

impl<B, Block, C, A, PR> ProposerFactory<A, B, C, PR>
where
    A: TransactionPool<Block = Block> + 'static,
    B: backend::Backend<Block> + Send + Sync + 'static,
    Block: BlockT,
    C: BlockBuilderProvider<B, Block, C>
        + HeaderBackend<Block>
        + ProvideRuntimeApi<Block>
        + Send
        + Sync
        + 'static,
    C::Api: ApiExt<Block, StateBackend = backend::StateBackendFor<B, Block>>
        + BlockBuilderApi<Block>
        + GearRuntimeApi<Block>,
{
    fn init_with_now(
        &mut self,
        parent_header: &<Block as BlockT>::Header,
        now: Box<dyn Fn() -> time::Instant + Send + Sync>,
    ) -> Proposer<B, Block, C, A, PR> {
        let parent_hash = parent_header.hash();

        info!(
            "🙌 Starting consensus session on top of parent {:?}",
            parent_hash
        );

        let proposer = Proposer::<_, _, _, _, PR> {
            spawn_handle: self.spawn_handle.clone(),
            client: self.client.clone(),
            parent_hash,
            parent_number: *parent_header.number(),
            transaction_pool: self.transaction_pool.clone(),
            now,
            metrics: self.metrics.clone(),
            default_block_size_limit: self.default_block_size_limit,
            soft_deadline_percent: self.soft_deadline_percent,
            telemetry: self.telemetry.clone(),
            _phantom: PhantomData,
            include_proof_in_block_size_estimation: self.include_proof_in_block_size_estimation,
        };

        proposer
    }
}

impl<A, B, Block, C, PR> sp_consensus::Environment<Block> for ProposerFactory<A, B, C, PR>
where
    A: TransactionPool<Block = Block> + 'static,
    B: backend::Backend<Block> + Send + Sync + 'static,
    Block: BlockT,
    C: BlockBuilderProvider<B, Block, C>
        + HeaderBackend<Block>
        + ProvideRuntimeApi<Block>
        + Send
        + Sync
        + 'static,
    C::Api: ApiExt<Block, StateBackend = backend::StateBackendFor<B, Block>>
        + BlockBuilderApi<Block>
        + GearRuntimeApi<Block>,
    PR: ProofRecording,
{
    type CreateProposer = future::Ready<Result<Self::Proposer, Self::Error>>;
    type Proposer = Proposer<B, Block, C, A, PR>;
    type Error = sp_blockchain::Error;

    fn init(&mut self, parent_header: &<Block as BlockT>::Header) -> Self::CreateProposer {
        future::ready(Ok(
            self.init_with_now(parent_header, Box::new(time::Instant::now))
        ))
    }
}

/// The proposer logic.
pub struct Proposer<B, Block: BlockT, C, A: TransactionPool, PR> {
    spawn_handle: Box<dyn SpawnNamed>,
    client: Arc<C>,
    parent_hash: Block::Hash,
    parent_number: <<Block as BlockT>::Header as HeaderT>::Number,
    transaction_pool: Arc<A>,
    now: Box<dyn Fn() -> time::Instant + Send + Sync>,
    metrics: PrometheusMetrics,
    default_block_size_limit: usize,
    include_proof_in_block_size_estimation: bool,
    soft_deadline_percent: Percent,
    telemetry: Option<TelemetryHandle>,
    _phantom: PhantomData<(B, PR)>,
}

impl<A, B, Block, C, PR> sp_consensus::Proposer<Block> for Proposer<B, Block, C, A, PR>
where
    A: TransactionPool<Block = Block> + 'static,
    B: backend::Backend<Block> + Send + Sync + 'static,
    Block: BlockT,
    C: BlockBuilderProvider<B, Block, C>
        + HeaderBackend<Block>
        + ProvideRuntimeApi<Block>
        + Send
        + Sync
        + 'static,
    C::Api: ApiExt<Block, StateBackend = backend::StateBackendFor<B, Block>>
        + BlockBuilderApi<Block>
        + GearRuntimeApi<Block>,
    PR: ProofRecording,
{
    type Transaction = backend::TransactionFor<B, Block>;
    type Proposal = Pin<
        Box<
            dyn Future<Output = Result<Proposal<Block, Self::Transaction, PR::Proof>, Self::Error>>
                + Send,
        >,
    >;
    type Error = sp_blockchain::Error;
    type ProofRecording = PR;
    type Proof = PR::Proof;

    fn propose(
        self,
        inherent_data: InherentData,
        inherent_digests: Digest,
        max_duration: time::Duration,
        block_size_limit: Option<usize>,
    ) -> Self::Proposal {
        let (tx, rx) = oneshot::channel();
        let spawn_handle = self.spawn_handle.clone();

        spawn_handle.spawn_blocking(
            "gear-authorship-proposer",
            None,
            Box::pin(async move {
                // leave some time for evaluation and block finalization (33%)
                let deadline = (self.now)() + (max_duration / 3) * 2;
                let res = self
                    .propose_with(inherent_data, inherent_digests, deadline, block_size_limit)
                    .await;
                if tx.send(res).is_err() {
                    trace!(target: "gear::authorship", "Could not send block production result to proposer!");
                }
            }),
        );

        async move { rx.await? }.boxed()
    }
}

/// If the block is full we will attempt to push at most
/// this number of transactions before quitting for real.
/// It allows us to increase block utilization.
const MAX_SKIPPED_TRANSACTIONS: usize = 8;

impl<A, B, Block, C, PR> Proposer<B, Block, C, A, PR>
where
    A: TransactionPool<Block = Block>,
    B: backend::Backend<Block> + Send + Sync + 'static,
    Block: BlockT,
    C: BlockBuilderProvider<B, Block, C>
        + HeaderBackend<Block>
        + ProvideRuntimeApi<Block>
        + Send
        + Sync
        + 'static,
    C::Api: ApiExt<Block, StateBackend = backend::StateBackendFor<B, Block>>
        + BlockBuilderApi<Block>
        + GearRuntimeApi<Block>,
    PR: ProofRecording,
{
    async fn propose_with(
        self,
        inherent_data: InherentData,
        inherent_digests: Digest,
        deadline: time::Instant,
        block_size_limit: Option<usize>,
    ) -> Result<Proposal<Block, backend::TransactionFor<B, Block>, PR::Proof>, sp_blockchain::Error>
    {
        let propose_with_start = time::Instant::now();
        let mut block_builder = BlockBuilderExt::new(
            self.client
                .new_block_at(self.parent_hash, inherent_digests, PR::ENABLED)?,
            self.client.runtime_api(),
            self.parent_hash,
        );

        let create_inherents_start = time::Instant::now();
        let inherents = block_builder.create_inherents(inherent_data)?;
        let create_inherents_end = time::Instant::now();

        self.metrics.report(|metrics| {
            metrics.create_inherents_time.observe(
                create_inherents_end
                    .saturating_duration_since(create_inherents_start)
                    .as_secs_f64(),
            );
        });

        for inherent in inherents {
            match block_builder.push(inherent) {
                Err(ApplyExtrinsicFailed(Validity(e))) if e.exhausted_resources() => {
                    warn!(target: "gear::authorship", "⚠️  Dropping non-mandatory inherent from overweight block.")
                }
                Err(ApplyExtrinsicFailed(Validity(e))) if e.was_mandatory() => {
                    error!(target: "gear::authorship",
                        "❌️ Mandatory inherent extrinsic returned error. Block cannot be produced."
                    );
                    return Err(ApplyExtrinsicFailed(Validity(e)));
                }
                Err(e) => {
                    warn!(target: "gear::authorship",
                        "❗️ Inherent extrinsic returned unexpected error: {}. Dropping.",
                        e
                    );
                }
                Ok(_) => {}
            }
        }

        // proceed with transactions
        // We calculate soft deadline used only in case we start skipping transactions.
        let now = (self.now)();
        let left = deadline.saturating_duration_since(now);
        let left_micros: u64 = left.as_micros().saturated_into();
        let soft_deadline =
            now + time::Duration::from_micros(self.soft_deadline_percent.mul_floor(left_micros));
        let block_timer = time::Instant::now();
        let mut skipped = 0;
        let mut unqueue_invalid = Vec::new();

        let mut t1 = self.transaction_pool.ready_at(self.parent_number).fuse();
        let mut t2 =
            futures_timer::Delay::new(deadline.saturating_duration_since((self.now)()) / 8).fuse();

        let mut pending_iterator = select! {
            res = t1 => res,
            _ = t2 => {
                log::warn!(target: "gear::authorship",
                    "Timeout fired waiting for transaction pool at block #{}. \
                    Proceeding with production.",
                    self.parent_number,
                );
                self.transaction_pool.ready()
            },
        };

        let block_size_limit = block_size_limit.unwrap_or(self.default_block_size_limit);

        debug!(target: "gear::authorship", "Attempting to push transactions from the pool.");
        debug!(target: "gear::authorship", "Pool status: {:?}", self.transaction_pool.status());
        let mut transaction_pushed = false;

        let end_reason = loop {
            let pending_tx = if let Some(pending_tx) = pending_iterator.next() {
                pending_tx
            } else {
                break EndProposingReason::NoMoreTransactions;
            };

            let now = (self.now)();
            if now > deadline {
                debug!(target: "gear::authorship",
                    "Consensus deadline reached when pushing block transactions, \
                    proceeding with proposing."
                );
                break EndProposingReason::HitDeadline;
            }

            let pending_tx_data = pending_tx.data().clone();
            let pending_tx_hash = pending_tx.hash().clone();

            let block_size =
                block_builder.estimate_block_size(self.include_proof_in_block_size_estimation);
            if block_size + pending_tx_data.encoded_size() > block_size_limit {
                pending_iterator.report_invalid(&pending_tx);
                if skipped < MAX_SKIPPED_TRANSACTIONS {
                    skipped += 1;
                    debug!(target: "gear::authorship",
                        "Transaction would overflow the block size limit, \
                         but will try {} more transactions before quitting.",
                        MAX_SKIPPED_TRANSACTIONS - skipped,
                    );
                    continue;
                } else if now < soft_deadline {
                    debug!(target: "gear::authorship",
                        "Transaction would overflow the block size limit, \
                         but we still have time before the soft deadline, so \
                         we will try a bit more."
                    );
                    continue;
                } else {
                    debug!(target: "gear::authorship", "Reached block size limit, proceeding with proposing.");
                    break EndProposingReason::HitBlockSizeLimit;
                }
            }

            trace!(target: "gear::authorship", "[{:?}] Pushing to the block.", pending_tx_hash);
            match block_builder.push(pending_tx_data) {
                Ok(()) => {
                    transaction_pushed = true;
                    debug!(target: "gear::authorship", "[{:?}] Pushed to the block.", pending_tx_hash);
                }
                Err(ApplyExtrinsicFailed(Validity(e))) if e.exhausted_resources() => {
                    pending_iterator.report_invalid(&pending_tx);
                    if skipped < MAX_SKIPPED_TRANSACTIONS {
                        skipped += 1;
                        debug!(target: "gear::authorship",
                            "Block seems full, but will try {} more transactions before quitting.",
                            MAX_SKIPPED_TRANSACTIONS - skipped,
                        );
                    } else if (self.now)() < soft_deadline {
                        debug!(target: "gear::authorship",
                            "Block seems full, but we still have time before the soft deadline, \
                             so we will try a bit more before quitting."
                        );
                    } else {
                        debug!(target: "gear::authorship", "Reached block weight limit, proceeding with proposing.");
                        break EndProposingReason::HitBlockWeightLimit;
                    }
                }
                Err(e) if skipped > 0 => {
                    pending_iterator.report_invalid(&pending_tx);
                    trace!(target: "gear::authorship",
                        "[{:?}] Ignoring invalid transaction when skipping: {}",
                        pending_tx_hash,
                        e
                    );
                }
                Err(e) => {
                    pending_iterator.report_invalid(&pending_tx);
                    debug!(target: "gear::authorship", "[{:?}] Invalid transaction: {}", pending_tx_hash, e);
                    unqueue_invalid.push(pending_tx_hash);
                }
            }
        };

        if matches!(end_reason, EndProposingReason::HitBlockSizeLimit) && !transaction_pushed {
            warn!(target: "gear::authorship",
                "Hit block size limit of `{}` without including any transaction!",
                block_size_limit,
            );
        }

        self.transaction_pool.remove_invalid(&unqueue_invalid);

        // Pushing a custom extrinsic that must run at the end of a block
        let custom_extrinsic = block_builder.create_terminal_extrinsic()?;

        debug!(target: "gear::authorship", "⚙️  Pushing Gear::run extrinsic into the block...");
        match block_builder.push(custom_extrinsic) {
            Ok(()) => {
                debug!(target: "gear::authorship", "⚙️  ... pushed to the block");
            }
            Err(ApplyExtrinsicFailed(Validity(e))) if e.exhausted_resources() => {
                warn!(target: "gear::authorship", "⚠️  Dropping terminal extrinsic from an overweight block.")
            }
            Err(e) => {
                error!(target: "gear::authorship",
                    "❗️ Terminal extrinsic returned an error: {}. Dropping.",
                    e
                );
            }
        }

        let (block, storage_changes, proof) = block_builder.build()?.into_inner();

        self.metrics.report(|metrics| {
            metrics
                .number_of_transactions
                .set(block.extrinsics().len() as u64);
            metrics
                .block_constructed
                .observe(block_timer.elapsed().as_secs_f64());

            metrics.report_end_proposing_reason(end_reason);
        });

        info!(
			"🎁 Prepared block for proposing at {} ({} ms) [hash: {:?}; parent_hash: {}; extrinsics ({}): [{}]]",
			block.header().number(),
			block_timer.elapsed().as_millis(),
			block.header().hash(),
			block.header().parent_hash(),
			block.extrinsics().len(),
			block.extrinsics()
				.iter()
				.map(|xt| BlakeTwo256::hash_of(xt).to_string())
				.collect::<Vec<_>>()
				.join(", ")
		);
        telemetry!(
            self.telemetry;
            CONSENSUS_INFO;
            "prepared_block_for_proposing";
            "number" => ?block.header().number(),
            "hash" => ?block.header().hash(),
        );

        let proof =
            PR::into_proof(proof).map_err(|e| sp_blockchain::Error::Application(Box::new(e)))?;

        let propose_with_end = time::Instant::now();
        self.metrics.report(|metrics| {
            metrics.create_block_proposal_time.observe(
                propose_with_end
                    .saturating_duration_since(propose_with_start)
                    .as_secs_f64(),
            );
        });

        Ok(Proposal {
            block,
            proof,
            storage_changes,
        })
    }
}
