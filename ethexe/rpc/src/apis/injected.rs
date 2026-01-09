// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::{RpcEvent, errors};
use dashmap::DashMap;
use ethexe_common::{
    HashOf,
    injected::{InjectedTransaction, RemovalNotification, RpcOrNetworkInjectedTx, SignedPromise},
};
use jsonrpsee::{
    PendingSubscriptionSink, SubscriptionMessage, SubscriptionSink,
    core::{RpcResult, SubscriptionResult, async_trait},
    proc_macros::rpc,
    types::error::ErrorObjectOwned,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// Determines whether the injected transaction was accepted by the main service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InjectedTransactionAcceptance {
    Accept,
    /// Transaction was rejected with a some error (tx is duplicated, or rpc-node lost connection).
    Reject(RejectionMessage),
}

/// Represents a rejection message for an injected transaction.
#[derive(Debug, Clone, Serialize, Deserialize, derive_more::Display)]
pub struct RejectionMessage(pub String);

impl From<anyhow::Error> for RejectionMessage {
    fn from(error: anyhow::Error) -> Self {
        Self(error.to_string())
    }
}

/// Type represents the result for injected transaction promise subscription.
pub(crate) type PromiseResult = Result<SignedPromise, RemovalNotification>;

#[cfg_attr(not(feature = "client"), rpc(server))]
#[cfg_attr(feature = "client", rpc(server, client))]
pub trait Injected {
    #[method(name = "injected_sendTransaction")]
    async fn send_transaction(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> RpcResult<InjectedTransactionAcceptance>;

    #[subscription(
        name = "injected_subscribeTransactionPromise",
        unsubscribe = "injected_unsubscribeTransactionPromise", 
        item = PromiseResult
    )]
    async fn send_transaction_and_watch(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> SubscriptionResult;
}

pub(crate) type SubscribersMap =
    DashMap<HashOf<InjectedTransaction>, oneshot::Sender<PromiseResult>>;

#[derive(Debug, Clone)]
pub struct InjectedApi {
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,
    promise_waiters: Arc<SubscribersMap>,
}

#[async_trait]
impl InjectedServer for InjectedApi {
    async fn send_transaction(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> RpcResult<InjectedTransactionAcceptance> {
        tracing::trace!(
            tx_hash = %transaction.tx.data().to_hash(),
            ?transaction,
            "Called injected_sendTransaction"
        );
        self.forward_transaction(transaction).await
    }

    async fn send_transaction_and_watch(
        &self,
        pending: PendingSubscriptionSink,
        transaction: RpcOrNetworkInjectedTx,
    ) -> SubscriptionResult {
        let tx_hash = transaction.tx.data().to_hash();
        tracing::trace!(%tx_hash, "Called injected_subscribeTransactionPromise");

        // Check, that transaction wasn't already send.
        if self.promise_waiters.get(&tx_hash).is_some() {
            tracing::warn!(tx_hash = ?tx_hash, "transaction was already sent");
            return Err(
                format!("transaction with the same hash was already sent: {tx_hash}").into(),
            );
        }

        let subscription_sink = match self.forward_transaction(transaction).await? {
            InjectedTransactionAcceptance::Accept => pending.accept().await.inspect_err(|err| {
                tracing::warn!(
                    "failed to accept subscription for injected transaction promise: {err}"
                );
            })?,
            InjectedTransactionAcceptance::Reject(error) => {
                tracing::trace!(
                    "subscription for injected transaction promise was rejected because of {error}"
                );
                pending.reject(errors::bad_request(error)).await;
                return Ok(());
            }
        };

        // Try accept subscription, if some errors occur, just log them and return error to client.

        let (promise_sender, promise_receiver) = oneshot::channel();
        self.promise_waiters.insert(tx_hash, promise_sender);
        self.spawn_promise_waiter(subscription_sink, promise_receiver, tx_hash);

        Ok(())
    }
}

impl InjectedApi {
    pub(crate) fn new(rpc_sender: mpsc::UnboundedSender<RpcEvent>) -> Self {
        Self {
            rpc_sender,
            promise_waiters: Arc::new(DashMap::new()),
        }
    }

    pub fn send_promise(&self, promise: SignedPromise) {
        let Some((_, waiter)) = self.promise_waiters.remove(&promise.data().tx_hash) else {
            tracing::warn!(promise = ?promise, "receive unregistered promise");
            return;
        };

        if let Err(promise) = waiter.send(Ok(promise)) {
            tracing::trace!(promise = ?promise, "rpc promise receiver dropped");
        }
    }

    pub fn notify_transactions_removed(&self, removals_info: Vec<RemovalNotification>) {
        removals_info.into_iter().for_each(|info| {
            if let Some((_, waiter)) = self.promise_waiters.remove(&info.tx_hash)
                && let Err(value) = waiter.send(Err(info))
            {
                tracing::trace!("rpc promise receiver dropped for removed tx: {:?}", value);
            }
        })
    }

    /// This function forwards [`RpcOrNetworkInjectedTx`] to main service and waits for its acceptance.
    async fn forward_transaction(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> Result<InjectedTransactionAcceptance, ErrorObjectOwned> {
        let tx_hash = transaction.tx.data().to_hash();
        let (response_sender, response_receiver) = oneshot::channel();

        let event = RpcEvent::InjectedTransaction {
            transaction,
            response_sender,
        };

        if let Err(err) = self.rpc_sender.send(event) {
            tracing::error!(
                "Failed to send `RpcEvent::InjectedTransaction` event task: {err}. \
                The receiving end in the main service might have been dropped."
            );
            return Err(errors::internal());
        }

        tracing::trace!(%tx_hash, "Accept transaction, waiting for promise");

        response_receiver.await.map_err(|e| {
            // No panic case, as a responsibility of the RPC API is fulfilled.
            // The dropped sender signalizes that the main service has crashed
            // or is malformed, so problems should be handled there.
            tracing::error!(
                "Response sender for the `RpcEvent::InjectedTransaction` was dropped: {e}"
            );
            errors::internal()
        })
    }

    // Spawns a task that waits for the promise and sends it to the client.
    fn spawn_promise_waiter(
        &self,
        sink: SubscriptionSink,
        receiver: oneshot::Receiver<PromiseResult>,
        tx_hash: HashOf<InjectedTransaction>,
    ) {
        // This clone is cheap, as it only increases the ref count.
        let promise_waiters = self.promise_waiters.clone();

        tokio::spawn(async move {
            // Waiting for promise or client disconnection.
            let promise = tokio::select! {
                result = receiver => match result {
                    Ok(promise) => {
                        promise_waiters.remove(&tx_hash);
                        promise
                    }
                    Err(_) => {
                        unreachable!("promise sender is owned by the api; it cannot be dropped before this point")
                    }
                },
                _ = sink.closed() => {
                    promise_waiters.remove(&tx_hash);
                    return;
                },
            };

            let promise_msg = match SubscriptionMessage::from_json(&promise) {
                Ok(msg) => msg,
                Err(err) => {
                    tracing::error!(
                        error = %err,
                        "failed to create `SubscriptionMessage` from json object"
                    );
                    return;
                }
            };

            if let Err(err) = sink.send(promise_msg).await {
                tracing::warn!(
                    tx_hash = ?tx_hash,
                    error = %err,
                    "failed to send subscription message"
                )
            }
        });
    }
}
