// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use common::Deconstructable;
use futures::{
    channel::oneshot,
    future,
    future::{Either, Future, FutureExt},
    select,
};
use futures_timer::Delay;
use log::{debug, error, info, trace, warn};
use pallet_gear_rpc_runtime_api::GearApi as GearRuntimeApi;
use parity_scale_codec::Encode;
use sc_block_builder::BlockBuilderApi;
use sc_telemetry::{CONSENSUS_INFO, TelemetryHandle, telemetry};
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool};
use sp_api::{ApiExt, ApiRef, CallApiAt, ProvideRuntimeApi};
use sp_blockchain::{ApplyExtrinsicFailed::Validity, Error::ApplyExtrinsicFailed, HeaderBackend};
use sp_consensus::{DisableProofRecording, EnableProofRecording, ProofRecording, Proposal};
use sp_core::traits::SpawnNamed;
use sp_inherents::InherentData;
use sp_runtime::{
    Digest, Percent, SaturatedConversion,
    traits::{BlakeTwo256, Block as BlockT, Hash as HashT, Header as HeaderT},
};
use std::{
    marker::PhantomData,
    ops::{Add, Deref},
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::block_builder::{BlockBuilder, BlockBuilderBuilder};
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

const LOG_TARGET: &str = "gear::authorship";

/// A unit type wrapper to express a duration multiplier.
#[derive(Clone, Copy)]
pub struct DurationMultiplier(pub f32);

impl Add for DurationMultiplier {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl DurationMultiplier {
    fn plus_one(self) -> Self {
        Self(self.0 + 1.0)
    }
}

/// Default deadline slippage used by [`Proposer`].
///
/// Can be overwritten by [`ProposerFactory::set_deadline_slippage`].
pub const DEFAULT_DEADLINE_SLIPPAGE: DurationMultiplier = DurationMultiplier(0.1);

/// Default extrinsics application deadline fraction used by [`Proposer`].
///
/// Equivalent to the `NORMAL_DISPATCH_WEIGHT_RATIO` in `Runtime`
/// Can be overwritten by [`ProposerFactory::set_deadline`].
pub const DEFAULT_DISPATCH_RATIO: DurationMultiplier = DurationMultiplier(0.25);

/// Default gas allowance for the pseudo-inherent.
///
/// Used to align the gas allowance in `Runtime` to avoid proposing deadline slippage.
pub const DEFAULT_GAS_ALLOWANCE: u64 = 750_000_000_000;

/// [`Proposer`] factory.
pub struct ProposerFactory<A, C, PR> {
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
    /// transactions which exhaust resources, we will conclude that the block is full.
    soft_deadline_percent: Percent,
    telemetry: Option<TelemetryHandle>,
    /// When estimating the block size, should the proof be included?
    include_proof_in_block_size_estimation: bool,
    /// Hard limit for the gas allowed to burn in one block.
    max_gas: Option<u64>,
    /// Block proposing deadline slippage.
    ///
    /// The value is used to compute the deadline slippage during block production
    /// that can be tolerated before the terminal `pseudo-inherent` is considered
    /// to be "taking too long" and dropped.
    deadline_slippage: DurationMultiplier,
    /// Dispatch ratio for deadline calculation.
    ///
    /// The share of the block proposing deadline that is allowed to be used for
    /// extrinsics application.
    dispatch_ratio: DurationMultiplier,
    /// phantom member to pin the `ProofRecording` type.
    _phantom: PhantomData<PR>,
}

impl<A, C> ProposerFactory<A, C, DisableProofRecording> {
    /// Create a new proposer factory.
    ///
    /// Proof recording will be disabled when using proposers built by this instance
    /// to build blocks.
    pub fn new(
        spawn_handle: impl SpawnNamed + 'static,
        client: Arc<C>,
        transaction_pool: Arc<A>,
        prometheus: Option<&PrometheusRegistry>,
        telemetry: Option<TelemetryHandle>,
        max_gas: Option<u64>,
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
            max_gas,
            deadline_slippage: DEFAULT_DEADLINE_SLIPPAGE,
            dispatch_ratio: DEFAULT_DISPATCH_RATIO,
            _phantom: PhantomData,
        }
    }
}

impl<A, C> ProposerFactory<A, C, EnableProofRecording> {
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
        max_gas: Option<u64>,
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
            max_gas,
            deadline_slippage: DEFAULT_DEADLINE_SLIPPAGE,
            dispatch_ratio: DEFAULT_DISPATCH_RATIO,
            _phantom: PhantomData,
        }
    }

    /// Disable the proof inclusion when estimating the block size.
    pub fn disable_proof_in_block_size_estimation(&mut self) {
        self.include_proof_in_block_size_estimation = false;
    }
}

impl<A, C, PR> ProposerFactory<A, C, PR> {
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
    /// transactions which exhaust resources, we will conclude that the block is full.
    ///
    /// Setting the value too low will significantly limit the amount of transactions
    /// we try in case they exhaust resources. Setting the value too high can
    /// potentially open a DoS vector, where many "exhaust resources" transactions
    /// are being tried with no success, hence block producer ends up creating an empty block.
    pub fn set_soft_deadline(&mut self, percent: Percent) {
        self.soft_deadline_percent = percent;
    }

    /// Set block proposing deadline slippage percentage.
    ///
    /// The default value is [`DEFAULT_DEADLINE_SLIPPAGE`].
    pub fn set_deadline_slippage(&mut self, multiplier: DurationMultiplier) {
        self.deadline_slippage = multiplier;
    }

    /// Set extrinsics application deadline share within block proposing.
    ///
    /// The default value is [`DEFAULT_DISPATCH_RATIO`].
    pub fn set_dispatch_ratio(&mut self, multiplier: DurationMultiplier) {
        self.dispatch_ratio = multiplier;
    }
}

impl<Block, C, A, PR> ProposerFactory<A, C, PR>
where
    A: TransactionPool<Block = Block> + 'static,
    Block: BlockT,
    C: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
    C::Api: ApiExt<Block> + BlockBuilderApi<Block> + GearRuntimeApi<Block> + Clone,
{
    pub(super) fn init_with_now(
        &mut self,
        parent_header: &<Block as BlockT>::Header,
        now: Box<dyn Fn() -> Instant + Send + Sync>,
    ) -> Proposer<Block, C, A, PR> {
        let parent_hash = parent_header.hash();

        info!("🙌 Starting consensus session on top of parent {parent_hash:?}");

        Proposer::<_, _, _, PR> {
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
            max_gas: self.max_gas,
            deadline_slippage: self.deadline_slippage,
            dispatch_ratio: self.dispatch_ratio,
            _phantom: PhantomData,
            include_proof_in_block_size_estimation: self.include_proof_in_block_size_estimation,
        }
    }
}

impl<A, Block, C, PR> sp_consensus::Environment<Block> for ProposerFactory<A, C, PR>
where
    A: TransactionPool<Block = Block> + 'static,
    Block: BlockT,
    C: HeaderBackend<Block> + ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync + 'static,
    C::Api:
        ApiExt<Block> + BlockBuilderApi<Block> + GearRuntimeApi<Block> + Clone + Deconstructable<C>,
    PR: ProofRecording,
{
    type CreateProposer = future::Ready<Result<Self::Proposer, Self::Error>>;
    type Proposer = Proposer<Block, C, A, PR>;
    type Error = sp_blockchain::Error;

    fn init(&mut self, parent_header: &<Block as BlockT>::Header) -> Self::CreateProposer {
        future::ready(Ok(self.init_with_now(parent_header, Box::new(Instant::now))))
    }
}

/// The proposer logic.
pub struct Proposer<Block: BlockT, C, A: TransactionPool, PR> {
    spawn_handle: Box<dyn SpawnNamed>,
    client: Arc<C>,
    parent_hash: Block::Hash,
    parent_number: <<Block as BlockT>::Header as HeaderT>::Number,
    transaction_pool: Arc<A>,
    now: Box<dyn Fn() -> Instant + Send + Sync>,
    metrics: PrometheusMetrics,
    default_block_size_limit: usize,
    include_proof_in_block_size_estimation: bool,
    soft_deadline_percent: Percent,
    telemetry: Option<TelemetryHandle>,
    max_gas: Option<u64>,
    deadline_slippage: DurationMultiplier,
    dispatch_ratio: DurationMultiplier,
    _phantom: PhantomData<PR>,
}

impl<A, Block, C, PR> sp_consensus::Proposer<Block> for Proposer<Block, C, A, PR>
where
    A: TransactionPool<Block = Block> + 'static,
    Block: BlockT,
    C: HeaderBackend<Block> + ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync + 'static,
    C::Api:
        ApiExt<Block> + BlockBuilderApi<Block> + GearRuntimeApi<Block> + Clone + Deconstructable<C>,
    PR: ProofRecording,
{
    type Proposal =
        Pin<Box<dyn Future<Output = Result<Proposal<Block, PR::Proof>, Self::Error>> + Send>>;
    type Error = sp_blockchain::Error;
    type ProofRecording = PR;
    type Proof = PR::Proof;

    fn propose(
        self,
        inherent_data: InherentData,
        inherent_digests: Digest,
        max_duration: Duration,
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
pub(super) const MAX_SKIPPED_TRANSACTIONS: usize = 5;

impl<A, Block, C, PR> Proposer<Block, C, A, PR>
where
    A: TransactionPool<Block = Block>,
    Block: BlockT,
    C: HeaderBackend<Block> + ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync + 'static,
    C::Api:
        ApiExt<Block> + BlockBuilderApi<Block> + GearRuntimeApi<Block> + Clone + Deconstructable<C>,
    PR: ProofRecording,
{
    async fn propose_with(
        self,
        inherent_data: InherentData,
        inherent_digests: Digest,
        deadline: Instant,
        block_size_limit: Option<usize>,
    ) -> Result<Proposal<Block, PR::Proof>, sp_blockchain::Error> {
        let block_timer = Instant::now();
        let mut block_builder = BlockBuilderBuilder::new(self.client.as_ref())
            .on_parent_block(self.parent_hash)
            .with_parent_block_number(self.parent_number)
            .with_proof_recording(PR::ENABLED)
            .with_inherent_digests(inherent_digests)
            .build()?;

        self.apply_inherents(&mut block_builder, inherent_data)?;

        // TODO call `after_inherents` and check if we should apply extrinsincs here
        // <https://github.com/paritytech/substrate/pull/14275/>

        let end_reason = self
            .apply_extrinsics(&mut block_builder, deadline, block_size_limit)
            .await?;

        let (block, storage_changes, proof) = block_builder.build()?.into_inner();
        let block_took = block_timer.elapsed();

        let proof =
            PR::into_proof(proof).map_err(|e| sp_blockchain::Error::Application(Box::new(e)))?;

        self.print_summary(&block, end_reason, block_took, block_timer.elapsed());
        Ok(Proposal {
            block,
            proof,
            storage_changes,
        })
    }

    /// Apply all inherents to the block.
    fn apply_inherents(
        &self,
        block_builder: &mut BlockBuilder<'_, Block, C>,
        inherent_data: InherentData,
    ) -> Result<(), sp_blockchain::Error> {
        let create_inherents_start = Instant::now();
        let inherents = block_builder.create_inherents(inherent_data)?;
        let create_inherents_end = Instant::now();

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
                    warn!(
                        target: LOG_TARGET,
                        "⚠️  Dropping non-mandatory inherent from overweight block."
                    )
                }
                Err(ApplyExtrinsicFailed(Validity(e))) if e.was_mandatory() => {
                    error!(
                        "❌️ Mandatory inherent extrinsic returned error. Block cannot be produced."
                    );
                    return Err(ApplyExtrinsicFailed(Validity(e)));
                }
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "❗️ Inherent extrinsic returned unexpected error: {e}. Dropping."
                    );
                }
                Ok(_) => {}
            }
        }
        Ok(())
    }

    /// Apply as many extrinsics as possible to the block.
    async fn apply_extrinsics(
        &self,
        block_builder: &mut BlockBuilder<'_, Block, C>,
        deadline: Instant,
        block_size_limit: Option<usize>,
    ) -> Result<EndProposingReason, sp_blockchain::Error> {
        // proceed with transactions
        // Duration until the "ultimate" deadline.
        let now = (self.now)();
        let remaining_proposal_duration = deadline.saturating_duration_since(now);
        // Calculate the max duration of the extrinsics application phase.
        let deadline_multiplier = self.dispatch_ratio + self.deadline_slippage;
        let left = remaining_proposal_duration.mul_f32(deadline_multiplier.0);
        // Adjusted hard deadline for extrinsics application.
        let extrinsics_hard_deadline = now + left;
        // Soft deadline used only in case we start skipping transactions.
        let left_micros: u64 = left.as_micros().saturated_into();
        let soft_deadline =
            now + Duration::from_micros(self.soft_deadline_percent.mul_floor(left_micros));
        let mut skipped = 0;
        let mut unqueue_invalid = Vec::new();

        let mut t1 = self.transaction_pool.ready_at(self.parent_number).fuse();
        let mut t2 =
            futures_timer::Delay::new(deadline.saturating_duration_since((self.now)()) / 8).fuse();

        let mut pending_iterator = select! {
            res = t1 => res,
            _ = t2 => {
                warn!(target: LOG_TARGET,
                    "Timeout fired waiting for transaction pool at block #{}. \
                    Proceeding with production.",
                    self.parent_number,
                );
                self.transaction_pool.ready()
            },
        };

        let block_size_limit = block_size_limit.unwrap_or(self.default_block_size_limit);

        debug!(target: LOG_TARGET, "Attempting to push transactions from the pool.");
        debug!(target: LOG_TARGET, "Pool status: {:?}", self.transaction_pool.status());
        let mut transaction_pushed = false;

        let end_reason = loop {
            let pending_tx = if let Some(pending_tx) = pending_iterator.next() {
                pending_tx
            } else {
                debug!(
                    target: LOG_TARGET,
                    "No more transactions, proceeding with proposing."
                );

                break EndProposingReason::NoMoreTransactions;
            };

            let now = (self.now)();
            if now > extrinsics_hard_deadline {
                debug!(
                    target: LOG_TARGET,
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
                    debug!(
                        target: LOG_TARGET,
                        "Transaction would overflow the block size limit, \
                     but will try {} more transactions before quitting.",
                        MAX_SKIPPED_TRANSACTIONS - skipped,
                    );
                    continue;
                } else if now < soft_deadline {
                    debug!(
                        target: LOG_TARGET,
                        "Transaction would overflow the block size limit, \
                     but we still have time before the soft deadline, so \
                     we will try a bit more."
                    );
                    continue;
                } else {
                    debug!(
                        target: LOG_TARGET,
                        "Reached block size limit, proceeding with proposing."
                    );
                    break EndProposingReason::HitBlockSizeLimit;
                }
            }

            trace!(target: LOG_TARGET, "[{pending_tx_hash:?}] Pushing to the block.");
            match block_builder.push(pending_tx_data) {
                Ok(()) => {
                    transaction_pushed = true;
                    debug!(target: LOG_TARGET, "[{pending_tx_hash:?}] Pushed to the block.");
                }
                Err(ApplyExtrinsicFailed(Validity(e))) if e.exhausted_resources() => {
                    pending_iterator.report_invalid(&pending_tx);
                    if skipped < MAX_SKIPPED_TRANSACTIONS {
                        skipped += 1;
                        debug!(target: LOG_TARGET,
                            "Block seems full, but will try {} more transactions before quitting.",
                            MAX_SKIPPED_TRANSACTIONS - skipped,
                        );
                    } else if (self.now)() < soft_deadline {
                        debug!(target: LOG_TARGET,
                            "Block seems full, but we still have time before the soft deadline, \
                             so we will try a bit more before quitting."
                        );
                    } else {
                        debug!(
                            target: LOG_TARGET,
                            "Reached block weight limit, proceeding with proposing."
                        );
                        break EndProposingReason::HitBlockWeightLimit;
                    }
                }
                Err(e) => {
                    pending_iterator.report_invalid(&pending_tx);
                    debug!(
                        target: LOG_TARGET,
                        "[{pending_tx_hash:?}] Invalid transaction: {e}"
                    );
                    unqueue_invalid.push(pending_tx_hash);
                }
            }
        };

        if matches!(end_reason, EndProposingReason::HitBlockSizeLimit) && !transaction_pushed {
            warn!(
                target: LOG_TARGET,
                "Hit block size limit of `{block_size_limit}` without including any transaction!",
            );
        }

        self.transaction_pool.remove_invalid(&unqueue_invalid);

        // Attempt to apply pseudo-inherent on top of the current overlay in a separate thread.
        // In case the timeout was hit at previous step, adjust the `max_gas`.
        let mut max_gas = self.max_gas;
        if matches!(end_reason, EndProposingReason::HitDeadline) {
            // Ideally, we want to let the pseudo-inherent to use at least the
            // DEFAULT_GAS_ALLOWANCE amount of gas. But if the remaining time is
            // too short, we will have to adjust the `max_gas` accordingly.
            let now = (self.now)();
            let left = deadline.saturating_duration_since(now);
            let relaxed_duration = left.mul_f32(self.deadline_slippage.plus_one().0);
            let relaxed_picos: u64 = relaxed_duration
                .as_nanos()
                .saturating_mul(1000)
                .saturated_into();
            max_gas = max_gas.map_or(Some(relaxed_picos.min(DEFAULT_GAS_ALLOWANCE)), |gas| {
                Some(gas.min(relaxed_picos.min(DEFAULT_GAS_ALLOWANCE)))
            });

            warn!(target: "gear::authorship",
                "Adjusted the GasAllowance to {} for the pseudo-inherent.",
                max_gas.unwrap_or(0),
            );
        }

        let client = self.client.clone();
        let parent_hash = self.parent_hash;
        let (extrinsics, api, _, version, _, estimated_header_size) =
            block_builder.clone().deconstruct();

        // We need the overlay changes and transaction storage cache to send to a new thread.
        // The cloned `RuntimeApi` object can't be sent to a new thread directly so we have to
        // break it down into parts (that are `Send`) and then reconstruct it in the new thread.
        // If changes applied successfully, the updated extrinsics and api parts will be sent back
        // to update the original block builder and finalize the block.
        let (_, api_params) = api.deref().clone().into_parts();

        let update_block = async move {
            let (tx, rx) = oneshot::channel();
            let spawn_handle = self.spawn_handle.clone();

            spawn_handle.spawn_blocking(
                "block-builder-push",
                None,
                Box::pin(async move {
                    debug!(target: "gear::authorship", "⚙️  Pushing Gear::run extrinsic into the block...");
                    let mut local_block_builder = BlockBuilder::<'_, Block, C>::from_parts(
                        extrinsics,
                        ApiRef::from(C::Api::from_parts(client.as_ref(), api_params)),
                        client.as_ref(),
                        version,
                        parent_hash,
                        estimated_header_size);
                    let outcome = local_block_builder.push_final(max_gas).map(|_| {
                        let (extrinsics, api, _, _, _, _) =
                        local_block_builder.deconstruct();
                        let (_, api_params) = api.deref().clone().into_parts();
                        (extrinsics, api_params)
                    });
                    if tx.send(outcome).is_err() {
                        warn!(
                            target: "gear::authorship",
                            "🔒 Send failure: the receiver must have already closed the channel.");
                    };
                }),
            );

            rx.await?
        }.boxed();
        match futures::future::select(
            update_block,
            // Allowing small deadline slippage.
            Delay::new(
                deadline
                    .add(remaining_proposal_duration.mul_f32(self.deadline_slippage.0))
                    .saturating_duration_since((self.now)()),
            ),
        )
        .await
        {
            Either::Left((res, _)) => {
                match res {
                    Ok((extrinsics, api_params)) => {
                        debug!(target: "gear::authorship", "⚙️  ... pushed to the block");
                        let mut api = C::Api::from_parts(self.client.as_ref(), api_params);
                        block_builder.set_api(&mut api);
                        block_builder.set_extrinsics(extrinsics);
                    }
                    Err(ApplyExtrinsicFailed(Validity(e))) if e.exhausted_resources() => {
                        warn!(target: "gear::authorship", "⚠️  Dropping terminal extrinsic from an overweight block.");
                    }
                    Err(e) => {
                        error!(target: "gear::authorship",
                            "❗️ Terminal extrinsic returned an error: {e}. Dropping."
                        );
                    }
                };
            }
            Either::Right(_) => {
                error!(
                    target: "gear::authorship",
                    "⌛️ Pseudo-inherent is taking too long and will not be included in the block."
                );
            }
        };

        Ok(end_reason)
    }

    /// Prints a summary and does telemetry + metrics.
    ///
    /// - `block`: The block that was build.
    /// - `end_reason`: Why did we stop producing the block?
    /// - `block_took`: How long did it took to produce the actual block?
    /// - `propose_took`: How long did the entire proposing took?
    fn print_summary(
        &self,
        block: &Block,
        end_reason: EndProposingReason,
        block_took: Duration,
        propose_took: Duration,
    ) {
        let extrinsics = block.extrinsics();
        self.metrics.report(|metrics| {
            metrics.number_of_transactions.set(extrinsics.len() as u64);
            metrics.block_constructed.observe(block_took.as_secs_f64());
            metrics.report_end_proposing_reason(end_reason);
            metrics
                .create_block_proposal_time
                .observe(propose_took.as_secs_f64());
        });

        let extrinsics_summary = if extrinsics.is_empty() {
            "no extrinsics".to_string()
        } else {
            format!(
                "extrinsics ({}): [{}]",
                extrinsics.len(),
                extrinsics
                    .iter()
                    .map(|xt| BlakeTwo256::hash_of(xt).to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        info!(
            "🎁 Prepared block for proposing at {} ({} ms) [hash: {:?}; parent_hash: {}; {extrinsics_summary}",
            block.header().number(),
            block_took.as_millis(),
            <Block as BlockT>::Hash::from(block.header().hash()),
            block.header().parent_hash(),
        );
        telemetry!(
            self.telemetry;
            CONSENSUS_INFO;
            "prepared_block_for_proposing";
            "number" => ?block.header().number(),
            "hash" => ?<Block as BlockT>::Hash::from(block.header().hash()),
        );
    }
}
