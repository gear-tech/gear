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

use anyhow::{anyhow, Result};
use ethexe_observer::ObserverStatus;
use ethexe_sequencer::SequencerStatus;
use ethexe_utils::metrics::register_globals;
use hyper::{
    http::StatusCode,
    server::Server,
    service::{make_service_fn, service_fn},
    Body, Request, Response,
};
pub use prometheus::{
    self,
    core::{
        AtomicF64 as F64, AtomicI64 as I64, AtomicU64 as U64, GenericCounter as Counter,
        GenericCounterVec as CounterVec, GenericGauge as Gauge, GenericGaugeVec as GaugeVec,
    },
    exponential_buckets, Error as PrometheusError, Histogram, HistogramOpts, HistogramVec, Opts,
    Registry,
};
use prometheus::{core::Collector, Encoder, TextEncoder};
use std::{
    net::SocketAddr,
    time::{Duration, Instant, SystemTime},
};
use tokio::{sync::watch::Receiver, time};

mod sourced;

pub use sourced::{MetricSource, SourcedCounter, SourcedGauge, SourcedMetric};

#[derive(Debug, Clone)]
pub struct PrometheusConfig {
    pub name: String,
    pub addr: SocketAddr,
    pub registry: Registry,
}

impl PrometheusConfig {
    /// Create a new config using the default registry.
    pub fn new_with_default_registry(name: String, addr: SocketAddr) -> Self {
        let labels = [("chain".into(), "ethexe-dev".into())].into();
        let registry = Registry::new_custom(None, Some(labels))
            .expect("this can only fail if prefix is empty string");

        Self {
            name,
            addr,
            registry,
        }
    }
}

pub struct MetricsService {
    metrics: PrometheusMetrics,
    updated: Instant,
}

impl MetricsService {
    pub fn new(config: &PrometheusConfig) -> Result<Self> {
        let svc = PrometheusMetrics::setup(&config.registry, &config.name)
            .map(|metrics| MetricsService {
                metrics,
                updated: Instant::now(),
            })
            .map_err(|e| anyhow!("Failed to create `MetricsService`: {e}"))?;

        Ok(svc)
    }

    pub async fn run(
        mut self,
        mut observer_status: Receiver<ObserverStatus>,
        mut sequencer_status: Option<Receiver<SequencerStatus>>,
    ) -> ! {
        let mut interval = time::interval(Duration::from_secs(6));

        loop {
            interval.tick().await;

            self.metrics.update(
                *observer_status.borrow_and_update(),
                sequencer_status.as_mut().map(|s| *s.borrow_and_update()),
            );
            self.updated = Instant::now();
        }
    }
}

struct PrometheusMetrics {
    eth_block_height: Gauge<U64>,
    pending_upload_code: Gauge<U64>,
    last_router_state: Gauge<U64>,
    aggregated_commitments: Gauge<U64>,
    submitted_code_commitments: Gauge<U64>,
    submitted_block_commitments: Gauge<U64>,
}

impl PrometheusMetrics {
    fn setup(registry: &Registry, name: &str) -> Result<Self, PrometheusError> {
        register(
            Gauge::<U64>::with_opts(
                Opts::new(
                    "ethexe_build_info",
                    "A metric with a constant '1' value labeled by name, version",
                )
                .const_label("name", name),
            )?,
            registry,
        )?
        .set(1);

        register_globals(registry)?;

        let start_time_since_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();

        register(
            Gauge::<U64>::new(
                "ethexe_process_start_time_seconds",
                "Number of seconds between the UNIX epoch and the moment the process started",
            )?,
            registry,
        )?
        .set(start_time_since_epoch.as_secs());

        Ok(Self {
            // generic internals
            eth_block_height: register(
                Gauge::<U64>::new(
                    "ethexe_eth_block_height",
                    "Block height info of the ethereum observer",
                )?,
                registry,
            )?,

            pending_upload_code: register(
                Gauge::<U64>::new(
                    "ethexe_pending_upload_code",
                    "Pending upload code events of the ethereum observer",
                )?,
                registry,
            )?,

            last_router_state: register(
                Gauge::<U64>::new(
                    "ethexe_last_router_state",
                    "Block height of the latest state of the router contract",
                )?,
                registry,
            )?,

            aggregated_commitments: register(
                Gauge::<U64>::new(
                    "ethexe_aggregated_commitments",
                    "Number of commitments aggregated in sequencer",
                )?,
                registry,
            )?,

            submitted_code_commitments: register(
                Gauge::<U64>::new(
                    "ethexe_submitted_code_commitments",
                    "Number of submitted code commitments in sequencer",
                )?,
                registry,
            )?,

            submitted_block_commitments: register(
                Gauge::<U64>::new(
                    "ethexe_submitted_block_commitments",
                    "Number of submitted block commitments in sequencer",
                )?,
                registry,
            )?,
        })
    }

    fn update(
        &mut self,
        observer_status: ObserverStatus,
        maybe_sequencer_status: Option<SequencerStatus>,
    ) {
        self.eth_block_height.set(observer_status.eth_block_number);
        self.pending_upload_code
            .set(observer_status.pending_upload_code);
        self.last_router_state
            .set(observer_status.last_router_state);

        if let Some(sequencer_status) = maybe_sequencer_status {
            self.aggregated_commitments
                .set(sequencer_status.aggregated_commitments);
            self.submitted_code_commitments
                .set(sequencer_status.submitted_code_commitments);
            self.submitted_block_commitments
                .set(sequencer_status.submitted_block_commitments);
        }
    }
}

pub fn register<T: Clone + Collector + 'static>(
    metric: T,
    registry: &Registry,
) -> Result<T, PrometheusError> {
    registry.register(Box::new(metric.clone()))?;
    Ok(metric)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Hyper internal error.
    #[error(transparent)]
    Hyper(#[from] hyper::Error),

    /// Http request error.
    #[error(transparent)]
    Http(#[from] hyper::http::Error),

    /// i/o error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Prometheus port {0} already in use.")]
    PortInUse(SocketAddr),
}

async fn request_metrics(req: Request<Body>, registry: Registry) -> Result<Response<Body>, Error> {
    if req.uri().path() == "/metrics" {
        let metric_families = registry.gather();
        let mut buffer = vec![];
        let encoder = TextEncoder::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();

        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", encoder.format_type())
            .body(Body::from(buffer))
            .map_err(Error::Http)
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found."))
            .map_err(Error::Http)
    }
}

/// Initializes the metrics context, and starts an HTTP server
/// to serve metrics.
pub async fn init_prometheus(prometheus_addr: SocketAddr, registry: Registry) -> Result<(), Error> {
    let listener = tokio::net::TcpListener::bind(&prometheus_addr)
        .await
        .map_err(|_| Error::PortInUse(prometheus_addr))?;

    init_prometheus_with_listener(listener, registry).await
}

/// Init prometheus using the given listener.
async fn init_prometheus_with_listener(
    listener: tokio::net::TcpListener,
    registry: Registry,
) -> Result<(), Error> {
    let listener = hyper::server::conn::AddrIncoming::from_listener(listener)?;
    log::info!(
        "〽️ Prometheus exporter started at {}",
        listener.local_addr()
    );

    let service = make_service_fn(move |_| {
        let registry = registry.clone();

        async move {
            Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| {
                request_metrics(req, registry.clone())
            }))
        }
    });

    let (signal, on_exit) = tokio::sync::oneshot::channel::<()>();
    let server = Server::builder(listener)
        .serve(service)
        .with_graceful_shutdown(async {
            let _ = on_exit.await;
        });

    let result = server.await.map_err(Into::into);

    // Gracefully shutdown server, otherwise the server does not stop if it has open connections
    let _ = signal.send(());

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::{Client, Uri};

    #[test]
    fn prometheus_works() {
        const METRIC_NAME: &str = "test_test_metric_name_test_test";

        let runtime = tokio::runtime::Runtime::new().expect("Creates the runtime");

        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .expect("Creates listener");

        let local_addr = listener.local_addr().expect("Returns the local addr");

        let registry = Registry::default();
        register(
            prometheus::Counter::new(METRIC_NAME, "yeah").expect("Creates test counter"),
            &registry,
        )
        .expect("Registers the test metric");

        runtime.spawn(init_prometheus_with_listener(listener, registry));

        runtime.block_on(async {
            let client = Client::new();

            let res = client
                .get(Uri::try_from(&format!("http://{}/metrics", local_addr)).expect("Parses URI"))
                .await
                .expect("Requests metrics");

            let buf = hyper::body::to_bytes(res)
                .await
                .expect("Converts body to bytes");

            let body = String::from_utf8(buf.to_vec()).expect("Converts body to String");
            assert!(body.contains(&format!("{} 0", METRIC_NAME)));
        });
    }
}
