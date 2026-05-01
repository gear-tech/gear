// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Prometheus integration for the ethexe node.
//!
//! This crate exposes the metrics endpoint used by the node and keeps a small
//! set of liveness gauges in sync with the contents of the local database.
//! It also acts as a bridge between the HTTP exporter and subsystems that own
//! metrics registries outside of the global recorder, such as libp2p.
//!
//! [`PrometheusService`] runs an HTTP server and yields [`PrometheusEvent`]s to
//! the parent service. When `/metrics` is requested, the service:
//! - refreshes liveness gauges derived from the latest committed announce,
//! - renders metrics from the global `metrics` recorder,
//! - asks the parent service for extra registry dumps,
//! - merges everything into a single Prometheus text response.

use anyhow::{Context as _, Result};
use ethexe_common::db::{
    AnnounceStorageRO, BlockMetaStorageRO, GlobalsStorageRO, OnChainStorageRO,
};
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
/// Liveness gauges derived from the latest committed announce in the database.
pub struct LivenessMetrics {
    /// Height of the block referenced by the latest committed announce.
    pub latest_committed_block_number: Gauge,
    /// Timestamp of the block referenced by the latest committed announce.
    pub latest_committed_block_timestamp: Gauge,
    /// Seconds between the latest synced block and the latest committed announce.
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

/// Refreshes liveness gauges from the latest committed announce stored in the database.
///
/// If the node has not committed any announce yet, the gauges are left unchanged.
fn update_liveness_metrics(db: Database, metrics: LivenessMetrics) {
    let Some(latest_committed_block_header) = db
        .block_meta(db.globals().latest_prepared_block_hash)
        .last_committed_announce
        .and_then(|a| db.announce(a))
        .and_then(|a| db.block_header(a.block_hash))
    else {
        return;
    };

    let time_since_latest_committed_secs = db
        .globals()
        .latest_synced_block
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
