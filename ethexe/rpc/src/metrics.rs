// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Metrics for the RPC server.

use futures::future::BoxFuture;
use jsonrpsee::{
    server::{MethodResponse, middleware::rpc::RpcServiceT},
    types::Request,
};
use metrics::{Counter, Gauge, Histogram};
use std::{collections::HashMap, sync::LazyLock, time::Instant};
use tower::Layer;

/// Default methods names tracked by [super::RpcMetricsLayer].
pub const TRACKED_METHODS: &[&str] = &[
    "injected_sendTransaction",
    "injected_sendTransactionAndWatch",
    "program_calculateReplyForHandle",
];

static METHODS_MAP: LazyLock<HashMap<&'static str, MethodMetrics>> = LazyLock::new(|| {
    TRACKED_METHODS
        .iter()
        .copied()
        .map(|method_name| {
            (
                method_name,
                MethodMetrics::new_with_labels(&[("method", method_name)]),
            )
        })
        .collect()
});

/// Unified bundle of metrics for RPC method.
/// [metrics_derive::Metrics] macro will register all metrics under the `ethexe_rpc_*` scope.
///
/// ## Must use
/// This object must be created using [MethodMetrics::new_with_labels] method.
/// This method will construct all metrics with provided unique label.
#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_rpc")]
pub struct MethodMetrics {
    #[metric(
        rename = "calls_started_total",
        describe = "Number of started RPC calls for the method"
    )]
    pub calls_started: Counter,
    #[metric(
            rename = "calls_finished_total",
            labels = [("status", "ok")],
            describe = "Number of successfully finished RPC calls for the method"
        )]
    pub calls_finished_ok: Counter,
    #[metric(
            rename = "calls_finished_total",
            labels = [("status", "error")],
            describe = "Number of failed RPC calls for the method"
        )]
    pub calls_finished_err: Counter,
    #[metric(
        rename = "call_duration_seconds",
        describe = "Latency of RPC calls for the method in seconds"
    )]
    pub calls_latency_seconds: Histogram,
    #[metric(
        rename = "calls_in_flight",
        describe = "Number of in-flight RPC calls for the method"
    )]
    pub calls_in_flight: Gauge,
}

/// The metrics for internal state of [crate::apis::InjectedApi].
#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_rpc_injected_api")]
pub struct InjectedApiMetrics {
    #[metric(
        rename = "active_promise_subscriptions",
        describe = "Number of active subscriptions for injected transaction's promise"
    )]
    pub injected_tx_active_subscriptions: Gauge,
}

/// Metrics layer for [jsonrpsee::server::RpcServiceBuilder].
/// Uses [RpcMetricsService] to wrap each request to metrics collection logic.
#[derive(Clone, Default)]
pub struct RpcMetricsLayer;

impl<S> Layer<S> for RpcMetricsLayer {
    type Service = RpcMetricsService<S>;

    fn layer(&self, service: S) -> Self::Service {
        RpcMetricsService { service }
    }
}

#[derive(Clone)]
pub struct RpcMetricsService<S> {
    service: S,
}

impl<'a, S> RpcServiceT<'a> for RpcMetricsService<S>
where
    S: RpcServiceT<'a> + Send + Sync,
    S::Future: Send + 'a,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        let Some(metrics) = METHODS_MAP.get(request.method_name()) else {
            return Box::pin(self.service.call(request));
        };

        let future = self.service.call(request);
        Box::pin(async move {
            metrics.calls_started.increment(1);
            metrics.calls_in_flight.increment(1);
            let _metrics_guard = scopeguard::guard((), |_| metrics.calls_in_flight.decrement(1));

            let started_at = Instant::now();

            let response = future.await;

            metrics
                .calls_latency_seconds
                .record(started_at.elapsed().as_secs_f64());
            match response.is_success() {
                true => metrics.calls_finished_ok.increment(1),
                false => metrics.calls_finished_err.increment(1),
            }

            response
        })
    }
}
