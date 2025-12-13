use metrics::{Counter, Gauge};

/// RPC-related metrics.
#[derive(Debug, Clone, Default)]
pub struct RpcApiMetrics {
    /// Metrics for injected API.
    pub(crate) injected: InjectedApiMetrics,
}

#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_rpc_block_api")]
pub struct BlockApiMetrics {
    /// Number of requests block headers.
    pub(crate) requested_blocks: Counter,
    /// Number of requested block events.
    pub(crate) requested_block_events: Counter,
    /// Number of requested block outcomes.
    pub(crate) requested_block_outcomes: Counter,
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

#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_rpc_latency")]
pub struct RpcApiLatency {
    // TODO: add latency metrics for all requests.
}
