// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::promise_manager::{PendingSubscriber, SubscriberId};
use ethexe_common::{
    HashOf,
    injected::{InjectedTransaction, SignedTxReceipt},
};
use jsonrpsee::{SubscriptionMessage, SubscriptionSink};
use tracing::{error, trace, warn};

async fn send_receipt(sink: &SubscriptionSink, receipt: &SignedTxReceipt) {
    match SubscriptionMessage::from_json(receipt) {
        Ok(message) => {
            if let Err(err) = sink.send(message).await {
                trace!("failed to send receipt, client disconnected: err={err}");
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
}

/// Spawns [PendingSubscriber] in tokio runtime.
///
/// On task finishing applies the `on_finish` function that is need to drop some data.
pub fn spawn_pending_subscriber<F>(
    sink: SubscriptionSink,
    subscriber: PendingSubscriber,
    on_finish: F,
) where
    F: FnOnce(HashOf<InjectedTransaction>, SubscriberId) + std::marker::Send + 'static,
{
    let (tx_hash, subscriber_id, receiver) = subscriber.into_parts();

    let _handle = tokio::spawn(async move {
        let _guard = scopeguard::guard((tx_hash, subscriber_id), |(tx_hash, subscriber_id)| {
            on_finish(tx_hash, subscriber_id)
        });

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

        send_receipt(&sink, receipt.as_ref()).await;
    });
}

/// Delivers an already-stored [SignedTxReceipt] to a subscription sink that arrived too late
/// to register as a pending watcher.
pub fn spawn_ready_receipt_subscriber(sink: SubscriptionSink, receipt: SignedTxReceipt) {
    let _handle = tokio::spawn(async move {
        send_receipt(&sink, &receipt).await;
    });
}
