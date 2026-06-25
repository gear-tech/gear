// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::{filter::PromiseSubscriptionFilter, promise_manager::PendingSubscriber};
use ethexe_common::{
    HashOf,
    injected::{InjectedTransaction, Promise},
};
use jsonrpsee::{SubscriptionMessage, SubscriptionSink};
use tokio::sync::broadcast;
use tracing::{error, trace, warn};

/// Spawns [PendingSubscriber] in tokio runtime.
///
/// On task finishing applies the `on_finish` function that is need to drop some data.
pub fn spawn_pending_subscriber<F>(
    sink: SubscriptionSink,
    subscriber: PendingSubscriber,
    on_finish: F,
) where
    F: FnOnce(HashOf<InjectedTransaction>) + std::marker::Send + 'static,
{
    let (tx_hash, receiver) = subscriber.into_parts();

    let _handle = tokio::spawn(async move {
        let _guard = scopeguard::guard(tx_hash, on_finish);

        // Waiting for the first one: promise, timeout_err, client disconnect error.
        let receipt = tokio::select! {
            result = receiver => match result {
                Ok(receipt_result) => match receipt_result {
                    Ok(receipt) => receipt,
                    Err(_err) => {
                        unreachable!("promise sender is owned by the server; it cannot be dropped before this point");
                    }
                },
                Err(_) => {
                    warn!("promise wasn't received in time, finish waiting");
                    return;
                }
            },
            _ = sink.closed() => {
                trace!("subscription closed by user, stop background task");
                return;
            }
        };

        match SubscriptionMessage::from_json(&receipt) {
            Ok(message) => {
                if let Err(err) = sink.send(message).await {
                    trace!("failed to send promise, client disconnected: err={err}");
                }
            }
            Err(err) => {
                error!(
                    ?receipt,
                    ?err,
                    "serialization error: failed to create `SubscriptionMessage` from receipt; this must never happen"
                );
            }
        }
    });
}

/// Spawns the background task driving a single promise subscription until the
/// client disconnects or the broadcast channel closes.
///
/// [`RpcService::receive_computed_promise`](crate::RpcService::receive_computed_promise)
/// pushes every newly computed `Promise` into the manager's broadcast. Each
/// active subscription owns its own [`broadcast::Receiver`] and forwards
/// matching promises to the JSON-RPC sink; the per-subscriber `filter` is
/// applied here, just before the sink, so the broadcast itself stays
/// filter-agnostic. There is no replay of historical promises — only promises
/// computed after the subscription was accepted are delivered. A slow
/// subscriber that lags is told how many it missed and keeps going; the stream
/// is never closed on `Lagged`.
pub fn spawn_promises_subscriber(
    sink: SubscriptionSink,
    mut receiver: broadcast::Receiver<Promise>,
    filter: Option<PromiseSubscriptionFilter>,
) {
    let _handle = tokio::spawn(async move {
        loop {
            let promise = tokio::select! {
                result = receiver.recv() => match result {
                    Ok(promise) => promise,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "promise subscriber lagged, skipping missed promises");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                _ = sink.closed() => {
                    trace!("promise subscription closed by user, stop background task");
                    return;
                }
            };

            if filter.as_ref().is_some_and(|f| !f.matches(&promise)) {
                continue;
            }

            match SubscriptionMessage::from_json(&promise) {
                Ok(message) => {
                    if let Err(err) = sink.send(message).await {
                        trace!("failed to send promise, client disconnected: err={err}");
                        return;
                    }
                }
                Err(err) => {
                    error!(
                        ?promise,
                        ?err,
                        "serialization error: failed to create `SubscriptionMessage` from promise; this must never happen"
                    );
                }
            }
        }
    });
}
