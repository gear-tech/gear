// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! JSON-RPC specific middleware.

use std::{
    net::IpAddr,
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::future::{BoxFuture, FutureExt};
use governor::{clock::Clock, Jitter};
use jsonrpsee::{
    server::middleware::rpc::RpcServiceT,
    types::{ErrorObject, Id, Request},
    MethodResponse,
};

mod method_limit;
mod metrics;
mod node_health;
mod rate_limit;

pub(crate) use method_limit::MethodLimiters;
pub use method_limit::RpcMethodLimit;
pub use metrics::*;
pub use node_health::*;
pub use rate_limit::*;

const MAX_JITTER: Duration = Duration::from_millis(50);
const MAX_RETRIES: usize = 10;

/// JSON-RPC middleware layer.
#[derive(Debug, Clone, Default)]
pub struct MiddlewareLayer {
    rate_limit: Option<RateLimit>,
    metrics: Option<Metrics>,
    method_limiters: Option<Arc<MethodLimiters>>,
    protocol: &'static str,
    trusted_client_ip: Option<IpAddr>,
}

impl MiddlewareLayer {
    /// Create an empty MiddlewareLayer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure node-wide method limits and call attribution.
    pub(crate) fn with_method_limiters(
        self,
        method_limiters: Arc<MethodLimiters>,
        protocol: &'static str,
        trusted_client_ip: IpAddr,
    ) -> Self {
        Self {
            rate_limit: self.rate_limit,
            metrics: self.metrics,
            method_limiters: Some(method_limiters),
            protocol,
            trusted_client_ip: Some(trusted_client_ip),
        }
    }

    /// Enable new rate limit middleware enforced per minute.
    pub fn with_rate_limit_per_minute(self, n: NonZeroU32) -> Self {
        Self {
            rate_limit: Some(RateLimit::per_minute(n)),
            ..self
        }
    }

    /// Enable metrics middleware.
    pub fn with_metrics(self, metrics: Metrics) -> Self {
        Self {
            metrics: Some(metrics),
            ..self
        }
    }

    /// Register a new websocket connection.
    pub fn ws_connect(&self) {
        if let Some(metrics) = self.metrics.as_ref() {
            metrics.ws_connect();
        }
    }

    /// Register that a websocket connection was closed.
    pub fn ws_disconnect(&self, now: Instant) {
        if let Some(metrics) = self.metrics.as_ref() {
            metrics.ws_disconnect(now);
        }
    }
}

impl<S> tower::Layer<S> for MiddlewareLayer {
    type Service = Middleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        Middleware {
            service,
            rate_limit: self.rate_limit.clone(),
            metrics: self.metrics.clone(),
            method_limiters: self.method_limiters.clone(),
            protocol: self.protocol,
            trusted_client_ip: self.trusted_client_ip,
        }
    }
}

/// JSON-RPC middleware that handles metrics
/// and rate-limiting.
///
/// These are part of the same middleware
/// because the metrics needs to know whether
/// a call was rate-limited or not because
/// it will impact the roundtrip for a call.
pub struct Middleware<S> {
    service: S,
    rate_limit: Option<RateLimit>,
    metrics: Option<Metrics>,
    method_limiters: Option<Arc<MethodLimiters>>,
    protocol: &'static str,
    trusted_client_ip: Option<IpAddr>,
}

impl<'a, S> RpcServiceT<'a> for Middleware<S>
where
    S: Send + Sync + RpcServiceT<'a> + Clone + 'static,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, req: Request<'a>) -> Self::Future {
        let now = Instant::now();

        if let Some(metrics) = self.metrics.as_ref() {
            metrics.on_call(&req);
        }

        let service = self.service.clone();
        let rate_limit = self.rate_limit.clone();
        let metrics = self.metrics.clone();
        let method_limiters = self.method_limiters.clone();
        let protocol = self.protocol;
        let trusted_client_ip = self.trusted_client_ip;

        async move {
            let method_name = req.method_name();
            let method_admission = if let Some(limiters) = method_limiters.as_ref() {
                match limiters.try_acquire(method_name) {
                    Ok(admission) => admission,
                    Err(reason) => {
                        let canonical_name = limiters.canonical(method_name);
                        let canonical = canonical_name.as_deref().unwrap_or(method_name);
                        let rp = reject_method_limit(req.id.clone());
                        if let Some(metrics) = metrics.as_ref() {
                            metrics.method_limit_rejected(canonical, reason.as_str());
                            metrics.on_response(&req, &rp, true, now);
                        }
                        log_rpc_call(
                            trusted_client_ip,
                            protocol,
                            method_name,
                            canonical,
                            Some(limiters.as_ref()),
                            now,
                            "limited",
                            reason.as_str(),
                        );
                        return rp;
                    }
                }
            } else {
                None
            };

            let canonical = method_admission
                .as_ref()
                .map(|admission| admission.canonical.as_ref())
                .unwrap_or(method_name);
            let method_in_flight = method_admission.as_ref().and_then(|_| {
                metrics.as_ref().map(|m| {
                    m.method_limit_admitted(canonical);
                    m.method_limit_in_flight(canonical)
                })
            });

            let mut is_rate_limited = false;

            if let Some(limit) = rate_limit.as_ref() {
                let mut attempts = 0;
                let jitter = Jitter::up_to(MAX_JITTER);

                loop {
                    if attempts >= MAX_RETRIES {
                        let rp = reject_too_many_calls(req.id.clone());
                        if let Some(metrics) = metrics.as_ref() {
                            metrics.on_response(&req, &rp, true, now);
                        }
                        log_rpc_call(
                            trusted_client_ip,
                            protocol,
                            method_name,
                            canonical,
                            method_limiters.as_deref(),
                            now,
                            "limited",
                            "connection_rate",
                        );
                        return rp;
                    }

                    if let Err(rejected) = limit.inner.check() {
                        tokio::time::sleep(jitter + rejected.wait_time_from(limit.clock.now()))
                            .await;
                    } else {
                        break;
                    }

                    is_rate_limited = true;
                    attempts += 1;
                }
            }

            let rp = service.call(req.clone()).await;
            if let Some(metrics) = metrics.as_ref() {
                metrics.on_response(&req, &rp, is_rate_limited, now);
            }
            log_rpc_call(
                trusted_client_ip,
                protocol,
                method_name,
                canonical,
                method_limiters.as_deref(),
                now,
                if rp.is_success() { "success" } else { "error" },
                "none",
            );

            // Keep both the method admission and in-flight gauge guard alive until
            // the service future has completed.
            let _ = method_in_flight;
            let _ = method_admission;
            rp
        }
        .boxed()
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "The arguments form one structured RPC trace event"
)]
fn log_rpc_call(
    trusted_client_ip: Option<IpAddr>,
    protocol: &'static str,
    method: &str,
    canonical: &str,
    method_limiters: Option<&MethodLimiters>,
    now: Instant,
    outcome: &'static str,
    limit_reason: &'static str,
) {
    if !log::log_enabled!(target: "rpc_calls", log::Level::Trace) {
        return;
    }

    let concurrency = method_limiters.and_then(|limiters| limiters.concurrency(method));
    let method_limit_configured = concurrency.is_some();
    let (in_flight, max_in_flight) = concurrency.unwrap_or((0, 0));
    log::trace!(
        target: "rpc_calls",
        "protocol={protocol}, client_ip={trusted_client_ip:?}, method={method:.128?}, canonical={canonical:.128?}, duration_us={}, outcome={outcome}, limit_reason={limit_reason}, method_limit_configured={method_limit_configured}, in_flight={in_flight}, max_in_flight={max_in_flight}",
        now.elapsed().as_micros(),
    );
}

fn reject_method_limit(id: Id) -> MethodResponse {
    MethodResponse::error(
        id,
        ErrorObject::owned(-32999, "RPC method limit exceeded", None::<()>),
    )
}

fn reject_too_many_calls(id: Id) -> MethodResponse {
    MethodResponse::error(
        id,
        ErrorObject::owned(-32999, "RPC rate limit exceeded", None::<()>),
    )
}
#[cfg(test)]
mod method_limit_tests {
    use std::{
        borrow::Cow,
        net::IpAddr,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        task::Poll,
    };

    use futures::{future::BoxFuture, FutureExt};
    use jsonrpsee::{
        server::middleware::rpc::RpcServiceT,
        types::{ErrorObject, Id, Request},
        MethodResponse,
    };
    use tower::Layer;

    use super::{method_limit::MethodLimitReason, *};

    #[derive(Clone)]
    struct FakeService {
        calls: Arc<AtomicUsize>,
        pending: bool,
    }

    impl FakeService {
        fn new(calls: Arc<AtomicUsize>, pending: bool) -> Self {
            Self { calls, pending }
        }
    }

    impl<'a> RpcServiceT<'a> for FakeService {
        type Future = BoxFuture<'a, MethodResponse>;

        fn call(&self, req: Request<'a>) -> Self::Future {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let id = req.id;
            let pending = self.pending;
            async move {
                if pending {
                    futures::future::pending::<()>().await;
                }
                MethodResponse::error(id, ErrorObject::owned(-1, "service called", None::<()>))
            }
            .boxed()
        }
    }

    fn request(method: &'static str, id: u64) -> Request<'static> {
        Request::new(Cow::Borrowed(method), None, Id::Number(id))
    }

    fn limiters(specs: &[&str]) -> Arc<MethodLimiters> {
        Arc::new(
            MethodLimiters::try_new(
                specs
                    .iter()
                    .map(|spec| spec.parse().expect("valid method limit"))
                    .collect(),
            )
            .unwrap(),
        )
    }

    fn layer<S>(limiters: Arc<MethodLimiters>, service: S) -> Middleware<S> {
        MiddlewareLayer::new()
            .with_method_limiters(limiters, "http", IpAddr::from([127, 0, 0, 1]))
            .layer(service)
    }

    #[tokio::test]
    async fn method_limit_shared_registry_contends_across_layers() {
        let limiters = limiters(&["state_getRuntimeVersion=100,1"]);
        let first_calls = Arc::new(AtomicUsize::new(0));
        let second_calls = Arc::new(AtomicUsize::new(0));
        let first = layer(
            limiters.clone(),
            FakeService::new(first_calls.clone(), true),
        );
        let second = layer(limiters, FakeService::new(second_calls.clone(), false));

        let mut first_call = first.call(request("state_getRuntimeVersion", 1));
        assert!(matches!(futures::poll!(first_call.as_mut()), Poll::Pending));
        let response = second.call(request("state_getRuntimeVersion", 2)).await;

        assert_eq!(response.as_error_code(), Some(-32999));
        assert!(response.to_result().contains("RPC method limit exceeded"));
        assert_eq!(first_calls.load(Ordering::SeqCst), 1);
        assert_eq!(second_calls.load(Ordering::SeqCst), 0);
        drop(first_call);
    }

    #[tokio::test]
    async fn method_limit_release_admits_after_immediate_rejection() {
        let limiters = limiters(&["state_getRuntimeVersion=100,1"]);
        let first = layer(
            limiters.clone(),
            FakeService::new(Arc::new(AtomicUsize::new(0)), true),
        );
        let second_calls = Arc::new(AtomicUsize::new(0));
        let second = layer(limiters, FakeService::new(second_calls.clone(), false));

        let mut first_call = first.call(request("state_getRuntimeVersion", 1));
        assert!(matches!(futures::poll!(first_call.as_mut()), Poll::Pending));
        assert_eq!(
            second
                .call(request("state_getRuntimeVersion", 2))
                .await
                .as_error_code(),
            Some(-32999)
        );

        drop(first_call);
        assert_eq!(
            second
                .call(request("state_getRuntimeVersion", 3))
                .await
                .as_error_code(),
            Some(-1)
        );
        assert_eq!(second_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn method_limit_aliases_share_one_budget() {
        let limiters = limiters(&["state_getRuntimeVersion,chain_getRuntimeVersion=100,1"]);
        let first = layer(
            limiters.clone(),
            FakeService::new(Arc::new(AtomicUsize::new(0)), true),
        );
        let alias_calls = Arc::new(AtomicUsize::new(0));
        let alias = layer(limiters, FakeService::new(alias_calls.clone(), false));

        let mut canonical_call = first.call(request("state_getRuntimeVersion", 1));
        assert!(matches!(
            futures::poll!(canonical_call.as_mut()),
            Poll::Pending
        ));
        assert_eq!(
            alias
                .call(request("chain_getRuntimeVersion", 2))
                .await
                .as_error_code(),
            Some(-32999)
        );

        drop(canonical_call);
        assert_eq!(
            alias
                .call(request("chain_getRuntimeVersion", 3))
                .await
                .as_error_code(),
            Some(-1)
        );
        assert_eq!(alias_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn method_limit_groups_are_independent_and_unconfigured_unlimited() {
        let limiters = limiters(&["alpha=100,1", "beta=100,1"]);
        let alpha = limiters
            .try_acquire("alpha")
            .expect("configured")
            .expect("admitted");
        assert!(matches!(
            limiters.try_acquire("alpha"),
            Err(MethodLimitReason::Concurrency)
        ));
        let beta = limiters
            .try_acquire("beta")
            .expect("configured")
            .expect("admitted");
        assert!(limiters
            .try_acquire("unconfigured")
            .expect("unconfigured")
            .is_none());
        drop(alpha);
        drop(beta);
    }

    #[tokio::test]
    async fn method_limit_rate_rejection_skips_service() {
        let limiters = limiters(&["state_getRuntimeVersion=1,2"]);
        let calls = Arc::new(AtomicUsize::new(0));
        let service = layer(limiters, FakeService::new(calls.clone(), false));

        assert_eq!(
            service
                .call(request("state_getRuntimeVersion", 1))
                .await
                .as_error_code(),
            Some(-1)
        );
        let response = service.call(request("state_getRuntimeVersion", 2)).await;
        assert_eq!(response.as_error_code(), Some(-32999));
        assert!(response.to_result().contains("RPC method limit exceeded"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn method_limit_active_without_old_rate_limiter_or_whitelist_bypass() {
        let limiters = limiters(&["state_getRuntimeVersion=100,1"]);
        let calls = Arc::new(AtomicUsize::new(0));
        let service = layer(limiters, FakeService::new(calls.clone(), true));

        let mut first_call = service.call(request("state_getRuntimeVersion", 1));
        assert!(matches!(futures::poll!(first_call.as_mut()), Poll::Pending));
        let response = service.call(request("state_getRuntimeVersion", 2)).await;

        assert_eq!(response.as_error_code(), Some(-32999));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        drop(first_call);
    }

    #[tokio::test]
    async fn method_limit_cancelling_admitted_call_releases_permit() {
        let limiters = limiters(&["state_getRuntimeVersion=100,1"]);
        let service = layer(
            limiters.clone(),
            FakeService::new(Arc::new(AtomicUsize::new(0)), true),
        );

        let mut call = service.call(request("state_getRuntimeVersion", 1));
        assert!(matches!(futures::poll!(call.as_mut()), Poll::Pending));
        drop(call);

        let admission = limiters
            .try_acquire("state_getRuntimeVersion")
            .expect("configured")
            .expect("permit released on cancellation");
        drop(admission);
    }

    #[tokio::test]
    async fn method_limit_each_direct_request_is_charged() {
        let limiters = limiters(&["state_getRuntimeVersion=100,1"]);
        let service = layer(
            limiters,
            FakeService::new(Arc::new(AtomicUsize::new(0)), true),
        );

        let mut first_call = service.call(request("state_getRuntimeVersion", 1));
        assert!(matches!(futures::poll!(first_call.as_mut()), Poll::Pending));
        assert_eq!(
            service
                .call(request("state_getRuntimeVersion", 2))
                .await
                .as_error_code(),
            Some(-32999)
        );
        drop(first_call);
    }
}
