// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! RPC middleware to collect prometheus metrics on RPC calls.

use std::time::Instant;

use jsonrpsee::{types::Request, MethodResponse};
use prometheus_endpoint::{
    register, Counter, CounterVec, Gauge, GaugeVec, HistogramOpts, HistogramVec, Opts,
    PrometheusError, Registry, U64,
};

/// Histogram time buckets in microseconds.
const HISTOGRAM_BUCKETS: [f64; 11] = [
    5.0,
    25.0,
    100.0,
    500.0,
    1_000.0,
    2_500.0,
    10_000.0,
    25_000.0,
    100_000.0,
    1_000_000.0,
    10_000_000.0,
];

/// Metrics for RPC middleware storing information about the number of requests started/completed,
/// calls started/completed and their timings.
#[derive(Debug, Clone)]
pub struct RpcMetrics {
    /// Histogram over RPC execution times.
    calls_time: HistogramVec,
    /// Number of calls started.
    calls_started: CounterVec<U64>,
    /// Number of calls completed.
    calls_finished: CounterVec<U64>,
    /// Number of calls admitted by a node-wide method limit.
    method_limit_admitted: CounterVec<U64>,
    /// Number of calls rejected by a node-wide method limit.
    method_limit_rejected: CounterVec<U64>,
    /// Number of calls currently in flight under a node-wide method limit.
    method_limit_in_flight: GaugeVec<U64>,
    /// Number of Websocket sessions opened.
    ws_sessions_opened: Option<Counter<U64>>,
    /// Number of Websocket sessions closed.
    ws_sessions_closed: Option<Counter<U64>>,
    /// Histogram over RPC websocket sessions.
    ws_sessions_time: HistogramVec,
}

impl RpcMetrics {
    /// Create an instance of metrics
    pub fn new(metrics_registry: Option<&Registry>) -> Result<Option<Self>, PrometheusError> {
        if let Some(metrics_registry) = metrics_registry {
            Ok(Some(Self {
				calls_time: register(
					HistogramVec::new(
						HistogramOpts::new(
							"substrate_rpc_calls_time",
							"Total time [μs] of processed RPC calls",
						)
						.buckets(HISTOGRAM_BUCKETS.to_vec()),
						&["protocol", "method", "is_rate_limited"],
					)?,
					metrics_registry,
				)?,
				calls_started: register(
					CounterVec::new(
						Opts::new(
							"substrate_rpc_calls_started",
							"Number of received RPC calls (unique un-batched requests)",
						),
						&["protocol", "method"],
					)?,
					metrics_registry,
				)?,
				calls_finished: register(
					CounterVec::new(
						Opts::new(
							"substrate_rpc_calls_finished",
							"Number of processed RPC calls (unique un-batched requests)",
						),
						&["protocol", "method", "is_error", "is_rate_limited"],
					)?,
					metrics_registry,
				)?,
				method_limit_admitted: register(
					CounterVec::new(
						Opts::new(
							"substrate_rpc_method_limit_admitted",
							"Number of RPC calls admitted by a node-wide method limit",
						),
						&["protocol", "method"],
					)?,
					metrics_registry,
				)?,
				method_limit_rejected: register(
					CounterVec::new(
						Opts::new(
							"substrate_rpc_method_limit_rejected",
							"Number of RPC calls rejected by a node-wide method limit",
						),
						&["protocol", "method", "reason"],
					)?,
					metrics_registry,
				)?,
				method_limit_in_flight: register(
					GaugeVec::new(
						Opts::new(
							"substrate_rpc_method_limit_in_flight",
							"Number of RPC calls currently in flight under a node-wide method limit",
						),
						&["protocol", "method"],
					)?,
					metrics_registry,
				)?,
				ws_sessions_opened: register(
					Counter::new(
						"substrate_rpc_sessions_opened",
						"Number of persistent RPC sessions opened",
					)?,
					metrics_registry,
				)?
				.into(),
				ws_sessions_closed: register(
					Counter::new(
						"substrate_rpc_sessions_closed",
						"Number of persistent RPC sessions closed",
					)?,
					metrics_registry,
				)?
				.into(),
				ws_sessions_time: register(
					HistogramVec::new(
						HistogramOpts::new(
							"substrate_rpc_sessions_time",
							"Total time [s] for each websocket session",
						)
						.buckets(HISTOGRAM_BUCKETS.to_vec()),
						&["protocol"],
					)?,
					metrics_registry,
				)?,
			}))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn ws_connect(&self) {
        if let Some(counter) = self.ws_sessions_opened.as_ref() {
            counter.inc();
        }
    }

    pub(crate) fn ws_disconnect(&self, now: Instant) {
        let micros = now.elapsed().as_secs();

        if let Some(counter) = self.ws_sessions_closed.as_ref() {
            counter.inc();
        }
        self.ws_sessions_time
            .with_label_values(&["ws"])
            .observe(micros as _);
    }

    pub(crate) fn on_call(&self, req: &Request, transport_label: &'static str) {
        log::trace!(
            target: "rpc_metrics",
            "[{transport_label}] on_call name={}",
            req.method_name(),
        );

        self.calls_started
            .with_label_values(&[transport_label, req.method_name()])
            .inc();
    }

    pub(crate) fn on_response(
        &self,
        req: &Request,
        rp: &MethodResponse,
        is_rate_limited: bool,
        transport_label: &'static str,
        now: Instant,
    ) {
        log::trace!(target: "rpc_metrics", "[{transport_label}] on_response started_at={:?}", now);

        let micros = now.elapsed().as_micros();
        log::debug!(
            target: "rpc_metrics",
            "[{transport_label}] {} call took {} μs",
            req.method_name(),
            micros,
        );
        self.calls_time
            .with_label_values(&[
                transport_label,
                req.method_name(),
                if is_rate_limited { "true" } else { "false" },
            ])
            .observe(micros as _);
        self.calls_finished
            .with_label_values(&[
                transport_label,
                req.method_name(),
                // the label "is_error", so `success` should be regarded as false
                // and vice-versa to be registered correctly.
                if rp.is_success() { "false" } else { "true" },
                if is_rate_limited { "true" } else { "false" },
            ])
            .inc();
    }

    pub(crate) fn method_limit_admitted(&self, method: &str, transport_label: &'static str) {
        self.method_limit_admitted
            .with_label_values(&[transport_label, method])
            .inc();
    }

    pub(crate) fn method_limit_rejected(
        &self,
        method: &str,
        reason: &str,
        transport_label: &'static str,
    ) {
        self.method_limit_rejected
            .with_label_values(&[transport_label, method, reason])
            .inc();
    }

    pub(crate) fn method_limit_in_flight(
        &self,
        method: &str,
        transport_label: &'static str,
    ) -> MethodLimitInFlightGuard {
        let gauge = self
            .method_limit_in_flight
            .with_label_values(&[transport_label, method]);
        gauge.inc();
        MethodLimitInFlightGuard { gauge }
    }
}

/// RAII guard for a method-limit in-flight metric.
pub(crate) struct MethodLimitInFlightGuard {
    gauge: Gauge<U64>,
}

impl Drop for MethodLimitInFlightGuard {
    fn drop(&mut self) {
        self.gauge.dec();
    }
}

/// Metrics with transport label.
#[derive(Clone, Debug)]
pub struct Metrics {
    pub(crate) inner: RpcMetrics,
    pub(crate) transport_label: &'static str,
}

impl Metrics {
    /// Create a new [`Metrics`].
    pub fn new(metrics: RpcMetrics, transport_label: &'static str) -> Self {
        Self {
            inner: metrics,
            transport_label,
        }
    }

    pub(crate) fn ws_connect(&self) {
        self.inner.ws_connect();
    }

    pub(crate) fn ws_disconnect(&self, now: Instant) {
        self.inner.ws_disconnect(now)
    }

    pub(crate) fn on_call(&self, req: &Request) {
        self.inner.on_call(req, self.transport_label)
    }

    pub(crate) fn on_response(
        &self,
        req: &Request,
        rp: &MethodResponse,
        is_rate_limited: bool,
        now: Instant,
    ) {
        self.inner
            .on_response(req, rp, is_rate_limited, self.transport_label, now)
    }
    pub(crate) fn method_limit_admitted(&self, method: &str) {
        self.inner
            .method_limit_admitted(method, self.transport_label)
    }

    pub(crate) fn method_limit_rejected(&self, method: &str, reason: &str) {
        self.inner
            .method_limit_rejected(method, reason, self.transport_label)
    }

    pub(crate) fn method_limit_in_flight(&self, method: &str) -> MethodLimitInFlightGuard {
        self.inner
            .method_limit_in_flight(method, self.transport_label)
    }
}

#[cfg(test)]
mod method_limit_metrics_tests {
    use std::{borrow::Cow, net::IpAddr, sync::Arc, task::Poll};

    use futures::{future::BoxFuture, FutureExt};
    use jsonrpsee::{
        server::middleware::rpc::RpcServiceT,
        types::{ErrorObject, Id, Request},
        MethodResponse,
    };
    use prometheus_endpoint::Registry;
    use tower::Layer;

    use crate::middleware::{MethodLimiters, MiddlewareLayer};

    use super::*;

    #[derive(Clone)]
    struct PendingService;

    impl<'a> RpcServiceT<'a> for PendingService {
        type Future = BoxFuture<'a, MethodResponse>;

        fn call(&self, req: Request<'a>) -> Self::Future {
            let id = req.id;
            async move {
                futures::future::pending::<()>().await;
                MethodResponse::error(id, ErrorObject::owned(-1, "unreachable", None::<()>))
            }
            .boxed()
        }
    }

    fn request(method: &'static str, id: u64) -> Request<'static> {
        Request::new(Cow::Borrowed(method), None, Id::Number(id))
    }

    #[tokio::test]
    async fn method_limit_metrics_follow_middleware_dispatch() {
        let registry = Registry::new();
        let rpc = RpcMetrics::new(Some(&registry))
            .expect("metrics registration succeeds")
            .expect("metrics are enabled");
        let limiters = Arc::new(
            MethodLimiters::try_new(vec!["state_getRuntimeVersion,chain_getRuntimeVersion=1,1"
                .parse()
                .expect("valid method limit")])
            .expect("method limit registration succeeds"),
        );
        let service = MiddlewareLayer::new()
            .with_method_limiters(limiters, "http", IpAddr::from([127, 0, 0, 1]))
            .with_metrics(Metrics::new(rpc.clone(), "http"))
            .layer(PendingService);

        let mut first = service.call(request("state_getRuntimeVersion", 1));
        assert!(matches!(futures::poll!(first.as_mut()), Poll::Pending));
        assert_eq!(
            rpc.method_limit_admitted
                .with_label_values(&["http", "state_getRuntimeVersion"])
                .get(),
            1
        );
        assert_eq!(
            rpc.method_limit_in_flight
                .with_label_values(&["http", "state_getRuntimeVersion"])
                .get(),
            1
        );

        let concurrency = service.call(request("chain_getRuntimeVersion", 2)).await;
        assert_eq!(concurrency.as_error_code(), Some(-32999));
        assert_eq!(
            rpc.method_limit_rejected
                .with_label_values(&["http", "state_getRuntimeVersion", "concurrency"])
                .get(),
            1
        );

        drop(first);
        assert_eq!(
            rpc.method_limit_in_flight
                .with_label_values(&["http", "state_getRuntimeVersion"])
                .get(),
            0
        );

        let rate = service.call(request("chain_getRuntimeVersion", 3)).await;
        assert_eq!(rate.as_error_code(), Some(-32999));
        assert_eq!(
            rpc.method_limit_rejected
                .with_label_values(&["http", "state_getRuntimeVersion", "rate"])
                .get(),
            1
        );
    }
}
