// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Ethexe Prometheus
//!
//! Prometheus metrics exporter for the ethexe node.
//!
//! This crate exposes a `/metrics` HTTP endpoint that Prometheus scrapers can query,
//! keeps a small set of liveness gauges ([`LivenessMetrics`]) in sync with the local
//! [`ethexe_db::Database`], and merges metric output from multiple registries — the
//! global `metrics` recorder and external subsystems (notably libp2p) — into a single
//! response text.
//!
//! ## Responsibilities
//!
//! - Installs the global `metrics` recorder and binds the HTTP exporter on construction.
//! - Refreshes `ethexe_liveness` gauges on every scrape from the latest committed MB
//!   stored in the database.
//! - Bridges the HTTP handler and external registry owners: on each `/metrics` request
//!   the service emits [`PrometheusEvent::CollectMetrics`] and waits for the caller to
//!   reply with additional Prometheus text before completing the response.
//! - Exposes two `pub static` channel-tracking instruments used across the workspace:
//!   [`UNBOUNDED_CHANNELS_COUNTER`] and [`UNBOUNDED_CHANNELS_SIZE`].
//!
//! ## Role in the stack
//!
//! `ethexe-prometheus` sits at the edge of the node process — it is a leaf in the
//! dependency graph that pulls read-only views from [`ethexe-db`] and [`ethexe-common`]
//! but is not depended on by any other ethexe crate except:
//!
//! - **`ethexe-service`** — owns an `Option<PrometheusService>`, polls it as a stream,
//!   and answers [`PrometheusEvent::CollectMetrics`] with the libp2p metrics text.
//! - **`ethexe-cli`** — constructs [`PrometheusConfig`] from CLI parameters.
//!
//! ```text
//! ethexe-service (stream consumer)
//!       │
//!       ▼
//! PrometheusService ──── HTTP /metrics ──→ Prometheus scraper
//!       │
//!       ├─ render global metrics recorder
//!       ├─ refresh LivenessMetrics from ethexe-db
//!       └─ request libp2p registry text via PrometheusEvent::CollectMetrics
//!              │
//!              └──→ ethexe-service ──→ network subsystem
//! ```
//!
//! ## Entry points / Public API
//!
//! [`PrometheusService::new`] is the single construction point.  It installs the global
//! recorder, initialises [`LivenessMetrics`], and spawns the HTTP server.
//!
//! ```rust,no_run
//! use ethexe_prometheus::{PrometheusConfig, PrometheusEvent, PrometheusService};
//! use futures::StreamExt as _;
//!
//! # async fn example(db: ethexe_db::Database) -> anyhow::Result<()> {
//! let config = PrometheusConfig {
//!     name: "node-1".into(),
//!     addr: "127.0.0.1:9635".parse()?,
//! };
//! let mut prometheus = PrometheusService::new(config, db)?;
//!
//! while let Some(event) = prometheus.next().await {
//!     match event {
//!         PrometheusEvent::CollectMetrics { libp2p_metrics } => {
//!             // Caller supplies extra registry text; may be dropped if network is off.
//!             let _ = libp2p_metrics.send(String::new());
//!         }
//!         PrometheusEvent::ServerClosed(_) => break,
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Key types
//!
//! - [`PrometheusService`] — `Stream<Item = PrometheusEvent>` + `FusedStream` wrapper
//!   around the background server task.
//! - [`PrometheusEvent`] — `CollectMetrics` (request extra registry text via a
//!   `oneshot`) or `ServerClosed` (task terminated).
//! - [`PrometheusConfig`] — `name` (exported as the global `node` label) and `addr`.
//! - [`LivenessMetrics`] — three `metrics::Gauge` fields under the `ethexe_liveness`
//!   scope: `latest_committed_block_number`, `latest_committed_block_timestamp`,
//!   `time_since_latest_committed_secs`.
//! - [`UNBOUNDED_CHANNELS_COUNTER`] / [`UNBOUNDED_CHANNELS_SIZE`] — `LazyLock` statics
//!   for instrumenting unbounded mpsc channels across the workspace.
//!
//! ## Invariants
//!
//! - At most one `PrometheusService` may be constructed per process; the second call to
//!   [`PrometheusService::new`] fails because a global recorder is already installed.
//! - The channel from the HTTP handler to `PrometheusService` must remain open for the
//!   lifetime of the server task; the handler panics (`expect`) if it is closed.
//! - The `FusedStream` implementation reports `is_terminated` only after
//!   `ServerClosed` has been yielded to the consumer, not merely when the task finishes.

use anyhow::{Context as _, Result};
use ethexe_common::db::{BlockMetaStorageRO, GlobalsStorageRO, MbStorageRO, OnChainStorageRO};
use ethexe_db::Database;
use futures::{FutureExt, Stream, stream::FusedStream};
use hyper::{
    Body, Request, Response, Server,
    http::StatusCode,
    server::conn::AddrIncoming,
    service::{make_service_fn, service_fn},
};
use metrics::Gauge;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use prometheus::{
    self, Opts,
    core::{AtomicU64, GenericCounterVec, GenericGaugeVec},
};
use std::{
    net::SocketAddr,
    pin::Pin,
    sync::LazyLock,
    task::{Context, Poll},
};
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot},
    task,
    task::JoinHandle,
};

/// Counts operations performed on tracked unbounded channels.
///
/// The `entity` label identifies the channel owner, while the `action` label
/// distinguishes sent, received, and dropped messages.
pub static UNBOUNDED_CHANNELS_COUNTER: LazyLock<GenericCounterVec<AtomicU64>> =
    LazyLock::new(|| {
        GenericCounterVec::new(
            Opts::new(
                "ethexe_unbounded_channel_len",
                "Items sent/received/dropped on each mpsc::unbounded instance",
            ),
            &["entity", "action"],
        )
        .expect("Creating of statics doesn't fail. qed")
    });

/// Tracks the current backlog size of tracked unbounded channels.
///
/// The `entity` label identifies the channel whose queue length is reported.
pub static UNBOUNDED_CHANNELS_SIZE: LazyLock<GenericGaugeVec<AtomicU64>> = LazyLock::new(|| {
    GenericGaugeVec::new(
        Opts::new(
            "ethexe_unbounded_channel_size",
            "Size (number of messages to be processed) of each mpsc::unbounded instance",
        ),
        &["entity"],
    )
    .expect("Creating of statics doesn't fail. qed")
});

#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_liveness")]
/// Liveness gauges derived from the latest committed MB in the database.
pub struct LivenessMetrics {
    /// Height of the block referenced by the latest committed MB.
    pub latest_committed_block_number: Gauge,
    /// Timestamp of the block referenced by the latest committed MB.
    pub latest_committed_block_timestamp: Gauge,
    /// Seconds between the latest synced block and the latest committed MB.
    pub time_since_latest_committed_secs: Gauge,
}

/// Configuration for the Prometheus service.
#[derive(Debug, Clone)]
pub struct PrometheusConfig {
    /// Value exported as the global `node` label on recorder-backed metrics.
    pub name: String,
    /// Address the HTTP exporter listens on.
    pub addr: SocketAddr,
}

#[derive(Debug)]
/// Events emitted by [`PrometheusService`] for integration with the parent node service.
pub enum PrometheusEvent {
    /// Requests additional metrics text from another subsystem before responding.
    CollectMetrics {
        /// One-shot channel used to return metrics in Prometheus text format.
        libp2p_metrics: oneshot::Sender<String>,
    },
    /// Signals that the background HTTP server task has terminated.
    ServerClosed(Result<(), task::JoinError>),
}

/// Stream wrapper around the background Prometheus HTTP server task.
///
/// The stream yields [`PrometheusEvent`] values until the server task exits.
pub struct PrometheusService {
    server: JoinHandle<()>,
    server_receiver: mpsc::Receiver<PrometheusEvent>,
    server_closed_returned: bool,
}

impl Stream for PrometheusService {
    type Item = PrometheusEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(res) = self.server.poll_unpin(cx) {
            self.server_closed_returned = true;
            return Poll::Ready(Some(PrometheusEvent::ServerClosed(res)));
        }

        if let Poll::Ready(Some(event)) = self.server_receiver.poll_recv(cx) {
            return Poll::Ready(Some(event));
        }

        Poll::Pending
    }
}

impl FusedStream for PrometheusService {
    fn is_terminated(&self) -> bool {
        self.server_closed_returned
    }
}

impl PrometheusService {
    /// Starts the Prometheus exporter and returns a stream of service events.
    ///
    /// This installs the global `metrics` recorder, initializes liveness gauges,
    /// and spawns the HTTP server bound to [`PrometheusConfig::addr`].
    pub fn new(config: PrometheusConfig, db: Database) -> Result<Self> {
        let handle = PrometheusBuilder::new()
            .add_global_label("node", config.name)
            .install_recorder()
            .context("Failed to install prometheus recorder")?;
        let metrics = LivenessMetrics::default();

        let (server_sender, server_receiver) = mpsc::channel(64);

        let server = tokio::spawn(
            start_prometheus_server(config.addr, server_sender, handle.clone(), metrics, db)
                .map(drop),
        );
        Ok(Self {
            server,
            server_receiver,
            server_closed_returned: false,
        })
    }
}

/// Runs the HTTP server that serves the Prometheus endpoint.
///
/// The server is shut down gracefully when its task is cancelled or finishes.
async fn start_prometheus_server(
    prometheus_addr: SocketAddr,
    sender: mpsc::Sender<PrometheusEvent>,
    handle: PrometheusHandle,
    metrics: LivenessMetrics,
    db: Database,
) -> Result<()> {
    let listener = TcpListener::bind(&prometheus_addr).await?;
    let listener = AddrIncoming::from_listener(listener)?;

    let (signal, on_exit) = oneshot::channel::<()>();

    let service = make_service_fn(move |_| {
        let sender = sender.clone();
        let handle = handle.clone();
        let metrics = metrics.clone();
        let db = db.clone();

        async move {
            Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| {
                request_metrics(
                    req,
                    sender.clone(),
                    handle.clone(),
                    metrics.clone(),
                    db.clone(),
                )
            }))
        }
    });

    let server = Server::builder(listener)
        .serve(service)
        .with_graceful_shutdown(async {
            let _ = on_exit.await;
        });

    log::info!("〽️ Prometheus exporter started at {prometheus_addr}");

    let result = server.await.map_err(Into::into);

    // Gracefully shutdown server, otherwise the server does not stop if it has open connections
    let _ = signal.send(());

    result
}

/// Handles an incoming HTTP request for the Prometheus exporter.
///
/// Requests to `/metrics` return the merged metrics payload. Any other path
/// receives a `404 Not Found` response.
async fn request_metrics(
    req: Request<Body>,
    sender: mpsc::Sender<PrometheusEvent>,
    handle: PrometheusHandle,
    metrics: LivenessMetrics,
    db: Database,
) -> Result<Response<Body>> {
    if req.uri().path() == "/metrics" {
        update_liveness_metrics(db, metrics);
        let mut metrics = handle.render();

        // we collect metrics from multiple registries
        debug_assert!(metrics.ends_with('\n'));
        debug_assert!(!metrics.ends_with("# EOF\n"));

        let (tx, rx) = oneshot::channel();
        sender
            .send(PrometheusEvent::CollectMetrics { libp2p_metrics: tx })
            .await
            .expect("channel must never be closed");

        // channel can be dropped if the network is disabled
        if let Ok(libp2p_metrics) = rx.await {
            metrics += &libp2p_metrics;
        }

        Response::builder()
            .status(StatusCode::OK)
            .header(
                hyper::header::CONTENT_TYPE,
                hyper::header::HeaderValue::from_static("text/plain"),
            )
            .body(Body::from(metrics))
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found."))
    }
    .context("Failed to request metrics")
}

/// Refreshes liveness gauges from the latest committed MB stored in the database.
///
/// If the node has not committed any MB yet, the gauges are left unchanged.
fn update_liveness_metrics(db: Database, metrics: LivenessMetrics) {
    let Some(latest_committed_block_header) = db
        .block_meta(db.globals().latest_prepared_eb_hash)
        .last_committed_mb
        .map(|mb_hash| db.mb_meta(mb_hash).last_advanced_eb)
        .and_then(|eth_block| db.block_header(eth_block))
    else {
        return;
    };

    let time_since_latest_committed_secs = db
        .globals()
        .latest_synced_eb
        .header
        .timestamp
        .saturating_sub(latest_committed_block_header.timestamp);

    metrics
        .latest_committed_block_number
        .set(latest_committed_block_header.height as f64);
    metrics
        .latest_committed_block_timestamp
        .set(latest_committed_block_header.timestamp as f64);
    metrics
        .time_since_latest_committed_secs
        .set(time_since_latest_committed_secs as f64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::{net::Ipv4Addr, time::Duration};
    use tokio::{task, time};

    #[tokio::test]
    async fn fused_stream_works() {
        let mut service = PrometheusService::new(
            PrometheusConfig {
                name: "".to_string(),
                addr: (Ipv4Addr::LOCALHOST, 0).into(),
            },
            Database::memory(),
        )
        .unwrap();

        assert!(!service.is_terminated());

        // wait for the server to finish
        time::timeout(Duration::from_secs(5), async {
            service.server.abort();
            while !service.server.is_finished() {
                task::yield_now().await;
            }
        })
        .await
        .unwrap();

        assert!(!service.is_terminated());

        let event = service.select_next_some().await;
        if let PrometheusEvent::ServerClosed(res) = event {
            assert!(res.unwrap_err().is_cancelled());
        } else {
            unreachable!("unexpected event: {event:?}");
        }

        assert!(service.is_terminated());
    }
}
