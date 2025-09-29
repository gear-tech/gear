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

use anyhow::{Context as _, Result};
use futures::{FutureExt, Stream, ready, stream::FusedStream};
use hyper::{
    Body, Request, Response, Server,
    http::StatusCode,
    server::conn::AddrIncoming,
    service::{make_service_fn, service_fn},
};
use prometheus::{
    self, Encoder, Opts, Registry, TextEncoder,
    core::{
        AtomicU64 as U64, AtomicU64, Collector, GenericCounterVec, GenericGauge as Gauge,
        GenericGaugeVec,
    },
};
use std::{
    net::SocketAddr,
    pin::Pin,
    sync::LazyLock,
    task::{Context, Poll},
    time::{Duration, Instant, SystemTime},
};
use tokio::{
    net::TcpListener,
    sync::oneshot,
    task::JoinHandle,
    time::{self, Interval},
};

/// Global metric for the number of unbounded channels.
pub static UNBOUNDED_CHANNELS_COUNTER: LazyLock<GenericCounterVec<AtomicU64>> =
    LazyLock::new(|| {
        GenericCounterVec::new(
            Opts::new(
                "gearexe_unbounded_channel_len",
                "Items sent/received/dropped on each mpsc::unbounded instance",
            ),
            &["entity", "action"],
        )
        .expect("Creating of statics doesn't fail. qed")
    });

/// Global metric for the size of unbounded channels.
pub static UNBOUNDED_CHANNELS_SIZE: LazyLock<GenericGaugeVec<AtomicU64>> = LazyLock::new(|| {
    GenericGaugeVec::new(
        Opts::new(
            "gearexe_unbounded_channel_size",
            "Size (number of messages to be processed) of each mpsc::unbounded instance",
        ),
        &["entity"],
    )
    .expect("Creating of statics doesn't fail. qed")
});

#[derive(Debug, Clone)]
/// Configuration for the Prometheus service.
pub struct PrometheusConfig {
    pub name: String,
    pub addr: SocketAddr,
    pub registry: Registry,
}

impl PrometheusConfig {
    /// Create a new config using the default registry.
    pub fn new(name: String, addr: SocketAddr) -> Self {
        let labels = [("chain".into(), "gearexe-dev".into())].into();

        let registry = Registry::new_custom(None, Some(labels))
            .expect("this can only fail if prefix is empty string");

        Self {
            name,
            addr,
            registry,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PrometheusEvent {
    CollectMetrics,
}

pub struct PrometheusService {
    metrics: PrometheusMetrics,
    updated: Instant,

    // to be used in stream impl.
    server: JoinHandle<()>,
    interval: Pin<Box<Interval>>,
}

impl Stream for PrometheusService {
    type Item = PrometheusEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let instant = ready!(self.interval.poll_tick(cx));

        self.updated = instant.into();

        Poll::Ready(Some(PrometheusEvent::CollectMetrics))
    }
}

impl FusedStream for PrometheusService {
    fn is_terminated(&self) -> bool {
        self.server.is_finished()
    }
}

impl PrometheusService {
    pub fn new(config: PrometheusConfig) -> Result<Self> {
        let metrics = PrometheusMetrics::setup(&config.registry, &config.name)
            .context("Failed to setup Prometheus metrics")?;

        let server = tokio::spawn(init_prometheus(config.addr, config.registry).map(drop));

        let interval = Box::pin(time::interval(Duration::from_secs(6)));

        Ok(Self {
            metrics,
            updated: Instant::now(),
            server,
            interval,
        })
    }

    pub fn update_observer_metrics(&mut self, eth_best_height: u32, pending_codes: usize) {
        self.metrics.eth_best_height.set(eth_best_height as u64);
        self.metrics.pending_codes.set(pending_codes as u64);
    }

    pub fn update_compute_metrics(
        &mut self,
        blocks_queue_len: usize,
        waiting_codes_count: usize,
        process_codes_count: usize,
    ) {
        self.metrics
            .compute_blocks_queue
            .set(blocks_queue_len as u64);
        self.metrics
            .compute_waiting_codes
            .set(waiting_codes_count as u64);
        self.metrics
            .compute_processing_codes
            .set(process_codes_count as u64);
    }
}

struct PrometheusMetrics {
    eth_best_height: Gauge<U64>,
    pending_codes: Gauge<U64>,
    compute_blocks_queue: Gauge<U64>,
    compute_waiting_codes: Gauge<U64>,
    compute_processing_codes: Gauge<U64>,
}

impl PrometheusMetrics {
    fn setup(registry: &Registry, name: &str) -> Result<Self> {
        register(
            Gauge::<U64>::with_opts(
                Opts::new(
                    "gearexe_build_info",
                    "A metric with a constant '1' value labeled by name, version",
                )
                .const_label("name", name),
            )?,
            registry,
        )?
        .set(1);

        registry.register(Box::new(UNBOUNDED_CHANNELS_COUNTER.clone()))?;
        registry.register(Box::new(UNBOUNDED_CHANNELS_SIZE.clone()))?;

        let start_time_since_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();

        register(
            Gauge::<U64>::new(
                "gearexe_start_time_since_epoch",
                "Number of seconds between the UNIX epoch and the moment the process started",
            )?,
            registry,
        )?
        .set(start_time_since_epoch.as_secs());

        Ok(Self {
            eth_best_height: register(
                Gauge::<U64>::new(
                    "gearexe_eth_best_height",
                    "Latest block height received by observer",
                )?,
                registry,
            )?,

            pending_codes: register(
                Gauge::<U64>::new(
                    "gearexe_pending_codes",
                    "Pending codes for lookup by observer",
                )?,
                registry,
            )?,

            compute_blocks_queue: register(
                Gauge::<U64>::new(
                    "gearexe_compute_blocks_queue",
                    "Number of blocks in the queue for processing",
                )?,
                registry,
            )?,

            compute_waiting_codes: register(
                Gauge::<U64>::new(
                    "gearexe_compute_waiting_codes",
                    "Number of codes waiting for loading to advance block processing",
                )?,
                registry,
            )?,

            compute_processing_codes: register(
                Gauge::<U64>::new(
                    "gearexe_compute_processing_codes",
                    "Number of processing codes",
                )?,
                registry,
            )?,
        })
    }
}

pub fn register<T: Clone + Collector + 'static>(metric: T, registry: &Registry) -> Result<T> {
    registry.register(Box::new(metric.clone()))?;
    Ok(metric)
}

async fn init_prometheus(prometheus_addr: SocketAddr, registry: Registry) -> Result<()> {
    let listener = TcpListener::bind(&prometheus_addr).await?;
    let listener = AddrIncoming::from_listener(listener)?;

    let (signal, on_exit) = oneshot::channel::<()>();

    let service = make_service_fn(move |_| {
        let registry = registry.clone();

        async move {
            Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| {
                request_metrics(req, registry.clone())
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

async fn request_metrics(req: Request<Body>, registry: Registry) -> Result<Response<Body>> {
    if req.uri().path() == "/metrics" {
        let metric_families = registry.gather();
        let mut buffer = vec![];
        let encoder = TextEncoder::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();

        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", encoder.format_type())
            .body(Body::from(buffer))
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found."))
    }
    .context("Failed to request metrics")
}
