// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use hyper::{
    Body, Request, Response,
    http::StatusCode,
    server::Server,
    service::{make_service_fn, service_fn},
};
pub use prometheus::{
    self, Error as PrometheusError, Histogram, HistogramOpts, HistogramVec, Opts, Registry,
    core::{
        AtomicF64 as F64, AtomicI64 as I64, AtomicU64 as U64, GenericCounter as Counter,
        GenericCounterVec as CounterVec, GenericGauge as Gauge, GenericGaugeVec as GaugeVec,
    },
    exponential_buckets,
};
use prometheus::{Encoder, TextEncoder, core::Collector};
use std::net::SocketAddr;

mod sourced;

pub use sourced::{MetricSource, SourcedCounter, SourcedGauge, SourcedMetric};

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
