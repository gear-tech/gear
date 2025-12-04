use metrics::{Counter, Gauge};

/// RPC-related metrics.
#[derive(Debug, Clone, Default)]
pub struct RpcApiMetrics {
    /// Metrics for injected API.
    pub(crate) injected: InjectedApiMetrics,
}

/// [`InjectedApiMetrics`] stores metrics for the injected RPC API.
#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_rpc_injected_api")]
pub struct InjectedApiMetrics {
    /// Number of sent injected transactions by`injected_sendTransaction` calls.
    pub(crate) transactions_sent: Counter,
    /// Number of promises given by `injected_subscribeTransactionPromise` calls.
    pub(crate) transaction_promises_sent: Counter,
    /// Number of currently active promise subscriptions.
    pub(crate) current_active_promise_subscriptions: Gauge,
}
