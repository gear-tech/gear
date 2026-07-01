// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::promise_manager::PendingSubscriber;
use ethexe_common::{HashOf, injected::InjectedTransaction};
use jsonrpsee::{SubscriptionMessage, SubscriptionSink};
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
