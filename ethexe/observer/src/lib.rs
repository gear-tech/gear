// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # ethexe-observer
//!
//! Watches the Ethereum chain head and back-fills missing blocks into the local database,
//! surfacing each arrival and each completed sync as an [`ObserverEvent`].
//!
//! ## Responsibilities
//!
//! - Subscribes to Ethereum block headers via a WebSocket provider and emits
//!   [`ObserverEvent::Block`] for every new head.
//! - Drives a per-head chain sync: walks back from the arrived head to the last
//!   persisted block, loads missing headers and decoded Router/Mirror events via
//!   [`EthereumBlockLoader`], writes them into [`ethexe_db::Database`], and emits
//!   [`ObserverEvent::BlockSynced`] when the gap is closed.
//! - Handles transient failures: subscription failures (drop or close) trigger
//!   exponential-backoff re-subscription. A [`SyncError::RpcError`] during sync is
//!   silently skipped: the error is logged, the recoverable-errors counter is
//!   incremented, and the observer moves on to the next queued head.
//!   [`SyncError::Fatal`] propagates as a stream error.
//! - Exposes Prometheus metrics for block-arrival latency, sync latency, and
//!   recoverable error count.
//!
//! ## Role in the Stack
//!
//! `ethexe-observer` sits at the bottom of the ethexe data-flow pipeline:
//!
//! ```text
//! Ethereum chain (WebSocket)
//!         │  block headers
//!         ▼
//!   ObserverService  ──ObserverEvent::Block──────────────────────────►  ethexe-service
//!         │  ChainSync (back-fill via EthereumBlockLoader)              (dispatches to
//!         │  reads Router/Mirror events from ethexe-ethereum            consensus, compute)
//!         │  writes headers + events to ethexe-db
//!         └──ObserverEvent::BlockSynced ──────────────────────────────►  ethexe-service
//! ```
//!
//! It is read-only with respect to Ethereum: all chain writes (batch commitments, etc.)
//! are the responsibility of `ethexe-ethereum`. Program execution is handled by
//! `ethexe-compute` and `ethexe-processor`. Consensus interpretation lives in
//! `ethexe-consensus`.
//!
//! ## Entry Points / Public API
//!
//! - [`ObserverService::new`] — async constructor; resolves the router address from
//!   the database config and queries the middleware address from the Router contract
//!   on-chain, connects the alloy provider, and starts the block-header subscription.
//! - [`ObserverService`] implements `futures::Stream<Item = Result<ObserverEvent>>` and
//!   `FusedStream`; `ethexe-service` polls it as the canonical chain-head source.
//! - [`ObserverService::provider`] — borrows the underlying `alloy` `RootProvider`.
//! - [`ObserverService::block_loader`] — returns a fresh [`EthereumBlockLoader`] bound
//!   to the configured router address.
//! - [`ObserverService::router_query`] — returns a fresh `RouterQuery` for read-only
//!   contract queries.
//!
//! ## Key Types
//!
//! | Type | Description |
//! |------|-------------|
//! | [`ObserverEvent`] | Stream item: `Block(SimpleBlockData)` on new head; `BlockSynced(H256)` after back-fill |
//! | [`ObserverConfig`] | Constructor input: Ethereum RPC URL and optional max sync depth |
//! | [`SyncError`] | Error classifier: `RpcError` (recoverable) vs `Fatal` (propagated) |
//! | [`utils::BlockLoader`] | Trait abstracting block-data loading from Ethereum |
//! | [`utils::EthereumBlockLoader`] | alloy-backed `BlockLoader` impl with chunked/concurrent log fetching |
//!
//! ## Invariants
//!
//! - The stream never reports itself as terminated: `FusedStream::is_terminated` always
//!   returns `false`.
//! - Back-fill terminates at the database watermark: the sync walk stops at the first
//!   block already recorded as synced, so the synced set must be contiguous from genesis.
//! - Block-hash continuity is asserted during the back-walk: if the loaded block's hash
//!   does not match the requested hash the code reaches `unreachable!`, enforcing that
//!   [`EthereumBlockLoader`] always returns data for the exact requested hash.
//! - Subscription-retry backoff is capped: `500 ms × 2^min(attempt, 6)`, maximum 30 s.
//! - `max_sync_depth` defaults to `u32::MAX` when `None` is passed in [`ObserverConfig`].
//!
//! ## Usage
//!
//! ```rust,no_run
//! use ethexe_observer::{ObserverConfig, ObserverEvent, ObserverService};
//! use futures::StreamExt as _;
//!
//! let mut observer = ObserverService::new(
//!     db.clone(),
//!     ObserverConfig { rpc: &ethereum_rpc_url, max_sync_depth: Some(1024) },
//! )
//! .await?;
//!
//! while let Some(event) = observer.next().await {
//!     match event? {
//!         ObserverEvent::Block(block)      => { /* new chain head */ }
//!         ObserverEvent::BlockSynced(hash) => { /* `hash` and ancestors now in db */ }
//!     }
//! }
//! ```

use crate::utils::EthereumBlockLoader;
use alloy::{
    providers::{Provider, ProviderBuilder, RootProvider},
    pubsub::{Subscription, SubscriptionStream},
    rpc::types::eth::Header,
    transports::TransportResult,
};
use anyhow::{Context as _, Result};
use ethexe_common::{
    Address, BlockHeader, ProtocolTimelines, SimpleBlockData, db::ConfigStorageRO,
};
use ethexe_db::Database;
use ethexe_ethereum::router::RouterQuery;
use futures::{FutureExt, Stream, StreamExt, future::BoxFuture, stream::FusedStream};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll, ready},
};
pub use sync::SyncError;
use sync::{ChainSync, SyncResult};

mod sync;
/// Utility types and traits for loading block data from the Ethereum chain.
pub mod utils;

#[cfg(test)]
mod tests;

type HeadersSubscriptionFuture = BoxFuture<'static, TransportResult<Subscription<Header>>>;

/// The wrapper on top of [`ChainSync::sync`] future.
/// It is needed to measure time taken for syncing a block.
type SyncFuture = future_timing::Timed<BoxFuture<'static, SyncResult<H256>>>;

/// Events emitted by [`ObserverService`] as it tracks the Ethereum chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObserverEvent {
    /// A new chain head was received; contains the raw block data.
    Block(SimpleBlockData),
    /// All blocks up to and including the given hash have been synced into the local database.
    BlockSynced(H256),
}

/// Configuration supplied to [`ObserverService::new`].
pub struct ObserverConfig<'a> {
    /// Ethereum RPC endpoint.
    pub rpc: &'a str,
    #[allow(rustdoc::private_intra_doc_links)]
    /// Maximum depth of blocks to sync, considered as u32::MAX if None,
    /// see also [`RuntimeConfig::max_sync_depth`].
    pub max_sync_depth: Option<u32>,
}

/// Metrics for the observer service.
/// The main purpose is to monitor the performance and health of the observer.
#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_observer")]
pub(crate) struct ObserverMetrics {
    /// The last Ethereum's block number.
    pub last_block_number: metrics::Gauge,
    /// The statistics about time for blocks arrival latency.
    pub blocks_latency: metrics::Histogram,
    /// The statistics about time for blocks syncing.
    pub block_syncing_latency: metrics::Histogram,
    /// Sync attempts that ended with a recoverable RPC error.
    pub recoverable_sync_errors: metrics::Counter,
}

#[derive(Clone, Debug)]
struct RuntimeConfig {
    /// Protocol timelines.
    timelines: ProtocolTimelines,
    /// Address of the Router contract.
    router_address: Address,
    /// Address of the Middleware contract.
    middleware_address: Address,
    /// Maximum depth of blocks to sync.
    max_sync_depth: u32,
    /// If block sync depth is greater than this value, blocks are synced in batches of this size.
    /// Must be greater than 1.
    batched_sync_depth: u32,
    /// Number of blocks after which election timestamp is considered finalized.
    finalization_period_blocks: u64,
}

// TODO #4552: make tests for observer service
/// Watches the Ethereum chain and surfaces each new head and completed back-fill as an [`ObserverEvent`].
///
/// Implements `futures::Stream<Item = Result<ObserverEvent>>`. Construct with [`ObserverService::new`]
/// and poll via `StreamExt::next`. The stream never terminates on its own; subscription failures
/// are retried with exponential backoff and recoverable RPC errors are skipped rather than propagated.
pub struct ObserverService {
    provider: RootProvider,
    config: RuntimeConfig,
    chain_sync: ChainSync,

    metrics: ObserverMetrics,
    headers_stream: SubscriptionStream<Header>,

    block_sync_queue: VecDeque<Header>,
    sync_future: Option<SyncFuture>,
    subscription_future: Option<HeadersSubscriptionFuture>,
    /// Exponent for the subscription-retry backoff. Bumped on every
    /// `subscribe_blocks` failure, cleared on success.
    subscription_retry_attempt: u32,
}

impl Stream for ObserverService {
    type Item = Result<ObserverEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // If subscription stream finished working, a new subscription is requested to be created.
        // The subscription creation request is a future itself, and it is polled here. If it's ready,
        // a new stream from it is created and used further to poll the next header.
        if let Some(future) = self.subscription_future.as_mut() {
            match ready!(future.as_mut().poll(cx)) {
                Ok(subscription) => {
                    self.headers_stream = subscription.into_stream();
                    self.subscription_future = None;
                    self.subscription_retry_attempt = 0;
                }
                Err(e) => {
                    log::warn!("observer: header subscription failed: {e:#}");
                    self.schedule_subscription_retry();
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            }
        }

        if let Poll::Ready(res) = self.headers_stream.poll_next_unpin(cx) {
            let Some(header) = res else {
                // Treat an unexpected stream end like a failed attempt:
                // a flapping endpoint can otherwise tight-loop us through
                // accept-then-immediate-close cycles with no sleep.
                log::warn!("observer: header stream ended unexpectedly");
                self.schedule_subscription_retry();
                cx.waker().wake_by_ref();
                return Poll::Pending;
            };

            self.metrics
                .blocks_latency
                .record(current_timestamp().saturating_sub(header.timestamp) as f64);
            self.metrics.last_block_number.set(header.number as f64);

            let data = SimpleBlockData {
                hash: H256(header.hash.0),
                header: BlockHeader {
                    height: header.number as u32,
                    timestamp: header.timestamp,
                    parent_hash: H256(header.parent_hash.0),
                },
            };

            log::trace!("Received a new block: {data:?}");
            self.block_sync_queue.push_front(header);

            return Poll::Ready(Some(Ok(ObserverEvent::Block(data))));
        }

        if self.sync_future.is_none()
            && let Some(header) = self.block_sync_queue.pop_back()
        {
            self.sync_future = Some(future_timing::timed(
                self.chain_sync.clone().sync(header).boxed(),
            ));
        }

        if let Some(fut) = self.sync_future.as_mut()
            && let Poll::Ready(timing_result) = fut.poll_unpin(cx)
        {
            let (timing, result) = timing_result.into_parts();
            self.metrics
                .block_syncing_latency
                .record((timing.busy() + timing.idle()).as_secs_f64());
            self.sync_future = None;

            match result {
                Ok(hash) => {
                    return Poll::Ready(Some(Ok(ObserverEvent::BlockSynced(hash))));
                }
                Err(SyncError::RpcError(err)) => {
                    log::warn!("observer: RPC error, retrying on next head: {err:#}");
                    self.metrics.recoverable_sync_errors.increment(1);
                    // Self-wake: queued headers may still be drainable.
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                Err(SyncError::Fatal(err)) => {
                    return Poll::Ready(Some(Err(err)));
                }
            }
        }

        Poll::Pending
    }
}

impl FusedStream for ObserverService {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl ObserverService {
    /// Creates a new `ObserverService`.
    ///
    /// Resolves the router address from the database config, queries the middleware address
    /// from the Router contract, connects the alloy provider, and starts the block-header subscription.
    pub async fn new(db: Database, config: ObserverConfig<'_>) -> Result<Self> {
        let ObserverConfig {
            rpc,
            max_sync_depth,
        } = config;

        let router_address = db.config().router_address;
        let router_query = RouterQuery::new(rpc, router_address).await?;
        let middleware_address = router_query.middleware_address().await?;

        let provider = ProviderBuilder::default()
            .connect(rpc)
            .await
            .context("failed to create ethereum provider")?;

        let headers_stream = provider
            .subscribe_blocks()
            .await
            .context("failed to subscribe blocks")?
            .into_stream();

        let config = RuntimeConfig {
            timelines: db.config().timelines,
            router_address,
            middleware_address,
            max_sync_depth: max_sync_depth.unwrap_or(u32::MAX),
            // TODO #4562: make this configurable.
            batched_sync_depth: 2,
            // TODO #4562: make this configurable, since different networks may have different finalization periods.
            finalization_period_blocks: 64,
        };

        let chain_sync = ChainSync::new(db, config.clone(), provider.clone());

        Ok(Self {
            provider,
            config,
            chain_sync,
            sync_future: None,
            block_sync_queue: VecDeque::new(),
            metrics: ObserverMetrics::default(),
            subscription_future: None,
            subscription_retry_attempt: 0,
            headers_stream,
        })
    }

    /// Returns a reference to the underlying alloy `RootProvider`.
    pub fn provider(&self) -> &RootProvider {
        &self.provider
    }

    /// Returns a fresh [`EthereumBlockLoader`] bound to the configured router address.
    pub fn block_loader(&self) -> EthereumBlockLoader {
        EthereumBlockLoader::new(self.provider.clone(), self.config.router_address)
    }

    /// Returns a fresh `RouterQuery` for read-only queries against the Router contract.
    pub fn router_query(&self) -> RouterQuery {
        RouterQuery::from_provider(self.config.router_address, self.provider.clone())
    }

    /// Arm `subscription_future` with the next exponential backoff before
    /// re-subscribing. Used by both the `Err` arm of an in-flight subscribe
    /// and the unexpected-stream-end branch — the latter would otherwise
    /// hammer the RPC if the provider accepts then immediately closes.
    fn schedule_subscription_retry(&mut self) {
        let attempt = self.subscription_retry_attempt.saturating_add(1);
        self.subscription_retry_attempt = attempt;
        let backoff = std::time::Duration::from_millis(
            (500u64.saturating_mul(1u64 << attempt.min(6))).min(30_000),
        );
        log::warn!("observer: re-subscribing to headers (attempt {attempt}, after {backoff:?})");
        self.metrics.recoverable_sync_errors.increment(1);
        let provider = self.provider().clone();
        self.subscription_future = Some(
            async move {
                tokio::time::sleep(backoff).await;
                provider.subscribe_blocks().await
            }
            .boxed(),
        );
    }
}

/// Function returns the current system timestamp in seconds.
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
