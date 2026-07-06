// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::promise_manager::PendingSubscriber;
use ethexe_common::injected::SignedTxReceipt;
use jsonrpsee::{SubscriptionMessage, SubscriptionSink};
use tracing::{error, trace, warn};

pub(crate) async fn send_receipt(sink: &SubscriptionSink, receipt: &SignedTxReceipt) {
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
/// `subscriber` itself is kept alive for the whole task instead of being torn
/// apart up front: its `Drop` impl removes the subscribers-map entry once its
/// receiver is actually gone, and that stays correct no matter which branch
/// below runs or whether the task is cancelled mid-wait.
///
/// `on_finish` is a secondary, best-effort hook (currently used for metrics)
/// and is not relied on for map cleanup.
pub fn spawn_pending_subscriber<F>(
    sink: SubscriptionSink,
    mut subscriber: PendingSubscriber,
    on_finish: F,
) where
    F: FnOnce() + std::marker::Send + 'static,
{
    let _handle = tokio::spawn(async move {
        let timeout = subscriber.timeout();

        // Waiting for the first one: receipt, timeout_err, client disconnect error.
        let receipt = tokio::select! {
            result = tokio::time::timeout(timeout, subscriber.receiver_mut().wait_for(|receipt| receipt.is_some())) => match result {
                Ok(Ok(receipt)) => Some(receipt.clone().expect("`wait_for` guarantees the receipt is set")),
                Ok(Err(_sender_dropped)) => {
                    warn!("receipt sender dropped before delivery, stop background task");
                    None
                }
                Err(_elapsed) => {
                    warn!("promise wasn't received in time, finish waiting");
                    None
                }
            },
            _ = sink.closed() => {
                trace!("subscription closed by user, stop background task");
                None
            }
        };

        // Free the subscribers-map entry now, before the final send, not at task end.
        drop(subscriber);

        if let Some(receipt) = receipt {
            send_receipt(&sink, receipt.as_ref()).await;
        }
        on_finish();
    });
}
