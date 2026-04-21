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

pub use metrics::*;
pub use middleware::{RpcMetricsLayer, RpcMetricsRegistry};

mod middleware {
    use super::metrics::{DEFAULT_TRACKED_METHODS, MethodMetrics};
    use futures::future::BoxFuture;
    use jsonrpsee::{
        server::{MethodResponse, middleware::rpc::RpcServiceT},
        types::Request,
    };
    use std::{collections::HashMap, sync::Arc, time::Instant};
    use tower::Layer;

    /// A methods metrics registry for [RpcMetricsLayer].
    /// Internally it uses the mapping `method_name` => [MethodMetrics], so the
    /// access to metrics is fast and do not add extra request latency.
    #[derive(Clone)]
    pub struct RpcMetricsRegistry {
        methods_map: Arc<HashMap<&'static str, MethodMetrics>>,
    }

    impl RpcMetricsRegistry {
        pub fn new(methods: &'static [&'static str]) -> Self {
            let mut methods_map = HashMap::new();
            methods.iter().copied().for_each(|method_name| {
                let method_metrics = MethodMetrics::new_with_labels(&[("method", method_name)]);
                methods_map.insert(method_name, method_metrics);
            });

            Self {
                methods_map: Arc::new(methods_map),
            }
        }

        pub fn get(&self, method: &str) -> Option<&MethodMetrics> {
            self.methods_map.get(method)
        }
    }

    impl Default for RpcMetricsRegistry {
        fn default() -> Self {
            Self::new(DEFAULT_TRACKED_METHODS)
        }
    }

    /// Metrics layer for [jsonrpsee::server::RpcServiceBuilder].
    /// Uses [RpcMetricsService] to wrap each request to metrics collection logic.
    ///
    /// Note: [Self::default] creates itself from registry with [DEFAULT_TRACKED_METHODS].
    #[derive(Clone, Default)]
    pub struct RpcMetricsLayer {
        registry: RpcMetricsRegistry,
    }

    impl RpcMetricsLayer {
        /// Creates new [RpcMetricsLayer] from registry.
        pub fn from_registry(registry: RpcMetricsRegistry) -> Self {
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

                metrics.on_incoming_request();
                let started_at = Instant::now();

                let response = future.await;
                metrics.on_outgoing_response(started_at, &response);
                response
            })
        }
    }
}

/// Metrics type definitions.
#[allow(clippy::module_inception)]
mod metrics {
    use jsonrpsee::server::MethodResponse;
    use metrics::{Counter, Gauge, Histogram};
    use std::time::Instant;

    /// Default methods names tracked by [super::RpcMetricsLayer].
    pub const DEFAULT_TRACKED_METHODS: &[&str] = &[
        "injected_sendTransaction",
        "injected_sendTransactionAndWatch",
        "program_calculateReplyForHandle",
    ];

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
        calls_started: Counter,
        #[metric(
            rename = "calls_finished_total",
            labels = [("status", "ok")],
            describe = "Number of successfully finished RPC calls for the method"
        )]
        calls_finished_ok: Counter,
        #[metric(
            rename = "calls_finished_total",
            labels = [("status", "error")],
            describe = "Number of failed RPC calls for the method"
        )]
        calls_finished_err: Counter,
        #[metric(
            rename = "call_duration_seconds",
            describe = "Latency of RPC calls for the method in seconds"
        )]
        calls_latency_seconds: Histogram,
        #[metric(
            rename = "calls_in_flight",
            describe = "Number of in-flight RPC calls for the method"
        )]
        calls_in_flight: Gauge,
    }

    impl MethodMetrics {
        pub fn on_incoming_request(&self) {
            self.calls_started.increment(1);
            self.calls_in_flight.increment(1);
        }

        pub fn on_outgoing_response(&self, started_at: Instant, response: &MethodResponse) {
            self.calls_latency_seconds
                .record(started_at.elapsed().as_secs_f64());

            match response.is_success() {
                true => self.calls_finished_ok.increment(1),
                false => self.calls_finished_err.increment(1),
            }

            self.calls_in_flight.decrement(1);
        }
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
}
