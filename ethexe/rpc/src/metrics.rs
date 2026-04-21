// This file is part of Gear.
//
// Copyright (C) 2025-2026 Gear Technologies Inc.
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

//! Metrics for the RPC server.

use futures::future::BoxFuture;
use jsonrpsee::{
    server::{MethodResponse, RpcServiceBuilder, middleware::rpc::RpcServiceT},
    types::Request,
};
use metrics::{Counter, Gauge, Histogram, counter, gauge, histogram};
use std::{collections::HashMap, sync::Arc, time::Instant};
use tower::{
    Layer,
    layer::util::{Identity, Stack},
};

/// Methods tracked by the generic RPC middleware.
pub const DEFAULT_TRACKED_METHODS: &[&str] = &[
    "injected_sendTransaction",
    "injected_sendTransactionAndWatch",
    "program_calculateReplyForHandle",
];

/// Metrics for the Injected RPC API lifecycle.
#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_rpc_injected_api")]
pub struct InjectedApiMetrics {
    /// The number of active injected transaction promises subscriptions.
    pub injected_tx_active_subscriptions: Gauge,
    /// The total number of injected transaction promises given to subscribers.
    pub injected_tx_promises_given: Counter,
}

#[derive(Clone)]
pub struct RpcMetricsRegistry {
    methods: Arc<HashMap<&'static str, MethodMetrics>>,
}

impl RpcMetricsRegistry {
    pub fn new(methods: &'static [&'static str]) -> Self {
        let methods = methods
            .iter()
            .copied()
            .map(|method| (method, MethodMetrics::new(method)))
            .collect();

        Self {
            methods: Arc::new(methods),
        }
    }

    fn get(&self, method: &str) -> Option<&MethodMetrics> {
        self.methods.get(method)
    }

    pub fn middleware(self) -> RpcServiceBuilder<Stack<RpcMetricsLayer, Identity>> {
        RpcServiceBuilder::new().layer(RpcMetricsLayer::new(self))
    }
}

#[derive(Clone)]
pub struct RpcMetricsLayer {
    registry: RpcMetricsRegistry,
}

impl RpcMetricsLayer {
    fn new(registry: RpcMetricsRegistry) -> Self {
        Self { registry }
    }
}

impl<S> Layer<S> for RpcMetricsLayer {
    type Service = RpcMetricsService<S>;

    fn layer(&self, service: S) -> Self::Service {
        RpcMetricsService {
            service,
            registry: self.registry.clone(),
        }
    }
}

#[derive(Clone)]
struct MethodMetrics {
    calls_started: Counter,
    calls_finished_ok: Counter,
    calls_finished_err: Counter,
    calls_latency_seconds: Histogram,
    calls_response_size_bytes: Histogram,
    calls_in_flight: Gauge,
}

impl MethodMetrics {
    fn new(method: &'static str) -> Self {
        Self {
            calls_started: counter!("ethexe_rpc_calls_started_total", "method" => method),
            calls_finished_ok: counter!(
                "ethexe_rpc_calls_finished_total",
                "method" => method,
                "status" => "ok"
            ),
            calls_finished_err: counter!(
                "ethexe_rpc_calls_finished_total",
                "method" => method,
                "status" => "error"
            ),
            calls_latency_seconds: histogram!(
                "ethexe_rpc_call_duration_seconds",
                "method" => method
            ),
            calls_response_size_bytes: histogram!(
                "ethexe_rpc_response_size_bytes",
                "method" => method
            ),
            calls_in_flight: gauge!("ethexe_rpc_calls_in_flight", "method" => method),
        }
    }
}

#[derive(Clone)]
pub struct RpcMetricsService<S> {
    service: S,
    registry: RpcMetricsRegistry,
}

impl<'a, S> RpcServiceT<'a> for RpcMetricsService<S>
where
    S: RpcServiceT<'a> + Send + Sync,
    S::Future: Send + 'a,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        let metrics = self.registry.get(request.method_name()).cloned();
        let future = self.service.call(request);

        Box::pin(async move {
            let Some(metrics) = metrics else {
                return future.await;
            };

            metrics.calls_started.increment(1);
            metrics.calls_in_flight.increment(1);
            let _in_flight_guard = scopeguard::guard(metrics.calls_in_flight.clone(), |gauge| {
                gauge.decrement(1);
            });
            let started_at = Instant::now();

            let response = future.await;

            metrics
                .calls_latency_seconds
                .record(started_at.elapsed().as_secs_f64());
            metrics
                .calls_response_size_bytes
                .record(response.as_result().len() as f64);

            if response.is_success() {
                metrics.calls_finished_ok.increment(1);
            } else {
                metrics.calls_finished_err.increment(1);
            }
            response
        })
    }
}
