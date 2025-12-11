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
use futures::{FutureExt, Stream, stream::FusedStream};
use hyper::{
    Body, Request, Response, Server,
    http::StatusCode,
    server::conn::AddrIncoming,
    service::{make_service_fn, service_fn},
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use prometheus::{
    self, Opts, Registry,
    core::{AtomicU64, GenericCounterVec, GenericGauge as Gauge, GenericGaugeVec},
};
use std::{
    net::SocketAddr,
    pin::Pin,
    sync::LazyLock,
    task::{Context, Poll},
    time::Instant,
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
                "ethexe_unbounded_channel_len",
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
            "ethexe_unbounded_channel_size",
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

pub struct PrometheusService {
    _updated: Instant,
    handle: PrometheusHandle,

    // to be used in stream impl.
    server: JoinHandle<()>,
}

impl Stream for PrometheusService {
    type Item = Result<()>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.server.poll_unpin(cx).map_err(Into::into).map(Some)
    }
}

impl FusedStream for PrometheusService {
    fn is_terminated(&self) -> bool {
        self.server.is_finished()
    }
}

impl PrometheusService {
    pub fn new(config: PrometheusConfig) -> Result<Self> {
        let handle = PrometheusBuilder::new()
            .install_recorder()
            .context("Failed to install prometheus recorder")?;

        let server = tokio::spawn(start_prometheus_server(config.addr, handle.clone()).map(drop));

        Ok(Self {
            _updated: Instant::now(),
            handle,
            server,
        })
    }
}

async fn start_prometheus_server(
    prometheus_addr: SocketAddr,
    handle: PrometheusHandle,
) -> Result<()> {
    let listener = TcpListener::bind(&prometheus_addr).await?;
    let listener = AddrIncoming::from_listener(listener)?;

    let (signal, on_exit) = oneshot::channel::<()>();

    let service = make_service_fn(move |_| {
        let handle = handle.clone();

        async move {
            Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| {
                request_metrics(req, handle.clone())
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

async fn request_metrics(req: Request<Body>, handle: PrometheusHandle) -> Result<Response<Body>> {
    if req.uri().path() == "/metrics" {
        let metrics = handle.render();

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
