// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::promise_manager::PendingSubscriber;
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
    F: FnOnce(HashOf<InjectedTransaction>) + std::marker::Send + 'static,
{
    let (tx_hash, mut receiver, timeout) = subscriber.into_parts();

    let _handle = tokio::spawn(async move {
        let _guard = scopeguard::guard(tx_hash, on_finish);

        // Waiting for the first one: receipt, timeout_err, client disconnect error.
        let receipt = tokio::select! {
            result = tokio::time::timeout(timeout, receiver.wait_for(|receipt| receipt.is_some())) => match result {
                Ok(Ok(receipt)) => receipt.clone().expect("`wait_for` guarantees the receipt is set"),
                Ok(Err(_sender_dropped)) => {
                    warn!("receipt sender dropped before delivery, stop background task");
                    return;
                }
                Err(_elapsed) => {
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
