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
    injected::{InjectedTransaction, RpcOrNetworkInjectedTx, SignedPromise},
};
use futures::StreamExt;
use jsonrpsee::{
    PendingSubscriptionSink, SubscriptionMessage, SubscriptionSink,
    core::{RpcResult, SubscriptionResult, async_trait},
    proc_macros::rpc,
    types::{ErrorObject, ErrorObjectOwned},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time::{Duration, timeout},
};
use tokio_stream::wrappers::BroadcastStream;

const MAX_PROMISE_CHANNEL_CAPACITY: usize = 1024;

/// The timeout for receiving the promise for an injected transaction.
/// Normally, the promise must be received within a few slots after the transactions submission.
const PROMISE_RECEIVING_TIMEOUT: Duration =
    Duration::from_secs(alloy::eips::merge::SLOT_DURATION_SECS * 10);

/// Determines whether the injected transaction was accepted by the main service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InjectedTransactionAcceptance {
    Accept,
}

#[cfg_attr(not(feature = "client"), rpc(server, namespace = "injected"))]
#[cfg_attr(feature = "client", rpc(server, client, namespace = "injected"))]
pub trait Injected {
    /// Just sends an injected transaction.
    #[method(name = "sendTransaction")]
    async fn send_transaction(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> RpcResult<InjectedTransactionAcceptance>;

    /// Sends an injected transaction and subscribes to its promise.  
    #[subscription(
        name = "sendTransactionAndWatch",
        unsubscribe = "sendTransactionAndWatchUnsubscribe", 
        item = SignedPromise
    )]
    async fn send_transaction_and_watch(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> SubscriptionResult;

    #[subscription(
        name = "injected_subscribePromises",
        unsubscribe = "injected_unsubscribePromises",
        item = SignedPromise
    )]
    async fn subscribe_promises(&self) -> SubscriptionResult;
}

/// Implementation of the injected transactions RPC API.
#[derive(derive_more::Debug, Clone)]
pub struct InjectedApi {
    /// Sender to forward RPC events to the main service.
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,
    #[debug(skip)]
    promise_manager: PromiseManager,
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
        tracing::trace!(%tx_hash, ?transaction, "Called injected_sendTransactionAndWatch");

        // Check if a waiter for the transaction promise already exists.
        if self.promise_manager.waiter_exists_for(&tx_hash) {
            tracing::trace!(tx_hash = ?tx_hash, "rejecting subscription: subscriber already exists");

            // Rejecting the subscription.
            pending
                .reject(errors::bad_request(
                    SubscriberAlreadyExistsError(tx_hash).to_string(),
                ))
                .await;
            return Ok(());
        }

        let _acceptance = self.forward_transaction(transaction).await?;

        let subscription_sink = pending.accept().await.inspect_err(|err| {
            tracing::warn!("failed to accept subscription for injected transaction promise: {err}")
        })?;

        tracing::trace!(?tx_hash, "Accept transaction, start promise waiter");

        // TODO kuzmindev: i am not sure about concurrency safety here.
        // Safe because we in a few lines above we checked that no existing waiter exists for the tx_hash.
        let promise_receiver = unsafe { self.promise_manager.register_waiter_unchecked(tx_hash) };
        self.spawn_promise_waiter(subscription_sink, promise_receiver, tx_hash);

        Ok(())
    }

    async fn subscribe_promises(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
        tracing::error!("Called injected_subscribePromises");
        let sink = pending.accept().await?;
        let mut stream = self.promise_manager.new_promise_stream();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = sink.closed() => {
                        tracing::error!("Promise subscription sink closed");
                        break
                    }
                    maybe_result = stream.next() => match maybe_result{
                        Some(Ok(promise)) => {
                            let Ok(msg) = subscription_message(&promise) else {
                                // Skip sending message if serialization fails.
                                continue;
                            };
                            if sink.send(msg).await.is_err() {
                                tracing::error!("promises stream subscriber disconnected, finishing subscription");
                                break;
                            }
                        }
                        Some(Err(err)) => {
                            tracing::error!(
                                "Promise subscription lagged by {err} messages",
                            );
                            // TODO kuzmindev: handle lagging case properly
                            continue
                        },
                        None => {
                            tracing::error!("Promise stream ended");
                            break
                        }
                    }
                }
            }
        });
        Ok(())
    }
}
impl InjectedApi {
    pub(crate) fn new(rpc_sender: mpsc::UnboundedSender<RpcEvent>) -> Self {
        Self {
            rpc_sender,
            promise_manager: PromiseManager::new(),
        }
    }

    pub(crate) fn send_promise(&self, promise: SignedPromise) {
        self.promise_manager.handle_promise(promise);
    }

    /// Returns the number of current promise subscribers waiting for promises.
    #[cfg(test)]
    pub fn promise_subscribers_count(&self) -> usize {
        self.promise_manager.promise_waiters.len()
    }

    /// This function forwards [`RpcOrNetworkInjectedTx`] to main service and waits for its acceptance.
    async fn forward_transaction(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> Result<InjectedTransactionAcceptance, ErrorObjectOwned> {
        let tx_hash = transaction.tx.data().to_hash();
        let (response_sender, response_receiver) = oneshot::channel();

        if transaction.tx.data().value != 0 {
            tracing::warn!(
                tx_hash = %tx_hash,
                value = transaction.tx.data().value,
                "Injected transaction with non-zero value is not supported"
            );
            return Err(errors::bad_request(
                "Injected transactions with non-zero value are not supported",
            ));
        }

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

        tracing::trace!(%tx_hash, "Accept transaction, waiting for acceptance");

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
        receiver: oneshot::Receiver<SignedPromise>,
        tx_hash: HashOf<InjectedTransaction>,
    ) {
        // This clone is cheap, as it only increases the ref count.
        let promises_manager = self.promise_manager.clone();

        tokio::spawn(async move {
            // Applying timeout to the receiver to avoid the infinite waiting for promise.
            let receiver = timeout(PROMISE_RECEIVING_TIMEOUT, receiver);

            // Waiting for promise, client disconnection or timeout.
            let promise = tokio::select! {
                result = receiver => {
                    promises_manager.remove_waiter(&tx_hash);
                    match result {
                        Ok(Ok(promise)) => promise,
                        Ok(Err(_)) => {
                            unreachable!("promise sender is owned by the api; it cannot be dropped before this point")
                        }
                        Err(_) => {
                            todo!()
                        }
                    }
                },
                _ = sink.closed() => {
                    promises_manager.remove_waiter(&tx_hash);
                    // promise_waiters.remove(&tx_hash);
                    return;
                },
            };

            let Ok(promise_msg) = subscription_message(&promise) else {
                return;
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

/// [`PromiseManager`] is responsible for delivering signed promises
/// to waiters for exact injected tx promise and broadcasting all
/// incoming promises to subscribers.
#[derive(Clone)]
struct PromiseManager {
    /// The waiters for exact injected tx promises.
    promise_waiters: Arc<DashMap<HashOf<InjectedTransaction>, oneshot::Sender<SignedPromise>>>,

    /// The broadcaster for promise subscribers ([`InjectedServer::subscribe_promises`]).
    promise_broadcaster: broadcast::Sender<SignedPromise>,
}

/// Error returned when a subscriber for an injected transaction already exists.
#[derive(thiserror::Error, Debug, Clone)]
#[error("Subscriber for injected transaction with hash {0} already exists")]
pub struct SubscriberAlreadyExistsError(HashOf<InjectedTransaction>);

impl PromiseManager {
    /// Creates a new instance of [`PromiseManager`].
    pub(crate) fn new() -> Self {
        let promise_broadcaster = broadcast::Sender::new(MAX_PROMISE_CHANNEL_CAPACITY);
        Self {
            promise_waiters: Arc::new(DashMap::new()),
            promise_broadcaster,
        }
    }

    /// Creates a new stream of signed promises for [`InjectedServer::subscribe_promises`].
    pub(crate) fn new_promise_stream(&self) -> BroadcastStream<SignedPromise> {
        tracing::trace!("Creating new promise stream for subscriber");
        BroadcastStream::new(self.promise_broadcaster.subscribe())
    }

    /// Handles an incoming signed promise.
    /// 1. If there is a waiter for the promise's transaction hash, sends the promise to it.
    /// 2. Broadcasts the promise to all subscribers.
    pub(crate) fn handle_promise(&self, promise: SignedPromise) {
        tracing::error!("PROMISE MANAGER: HANDLE PROMISE: {promise:?}");
        let tx_hash = promise.data().tx_hash;

        // Send to specific waiter if exists.
        if let Some((_, waiter)) = self.promise_waiters.remove(&tx_hash) {
            match waiter.send(promise.clone()) {
                Ok(()) => {
                    tracing::trace!(%tx_hash, ?promise, "successfully send promise to waiter");
                }
                Err(promise) => {
                    tracing::trace!(%tx_hash, ?promise, "failed to send promise because waiter dropped");
                }
            }
        }

        // Broadcast to all subscribers.
        match self.promise_broadcaster.send(promise) {
            Ok(receivers_count) => {
                tracing::error!("promise broadcasted to {receivers_count} subscribers");
            }
            Err(err) => {
                tracing::error!(
                    "there are no subscribers to receive the broadcasted promise: {err}",
                );
            }
        }
    }

    /// Checks if a waiter exists for the given transaction hash.
    pub(crate) fn waiter_exists_for(&self, tx_hash: &HashOf<InjectedTransaction>) -> bool {
        self.promise_waiters.contains_key(tx_hash)
    }

    /// Registers a new promise waiter for the given transaction hash.
    ///
    /// Returns an error if a waiter for the given transaction hash already exists.
    #[allow(unused)]
    pub(crate) fn register_waiter(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> Result<oneshot::Receiver<SignedPromise>, SubscriberAlreadyExistsError> {
        if self.waiter_exists_for(&tx_hash) {
            return Err(SubscriberAlreadyExistsError(tx_hash));
        }

        // Safe because we just checked that no existing waiter exists.
        Ok(unsafe { self.register_waiter_unchecked(tx_hash) })
    }

    /// Registers a new promise waiter for the given transaction hash without checking for existing ones.
    ///
    /// NOTE: This method must be call after [`Self::waiter_exists_for`] check to avoid overwriting existing waiters.
    pub(crate) unsafe fn register_waiter_unchecked(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> oneshot::Receiver<SignedPromise> {
        let (sender, receiver) = oneshot::channel();
        self.promise_waiters.insert(tx_hash, sender);
        receiver
    }

    /// Remove the promise waiter for the given transaction hash.
    pub(crate) fn remove_waiter(&self, tx_hash: &HashOf<InjectedTransaction>) -> bool {
        self.promise_waiters.remove(tx_hash).is_some()
    }
}

/// Helper function to create a subscription message from serializable data.
fn subscription_message<T>(data: &T) -> Result<SubscriptionMessage, ErrorObjectOwned>
where
    T: Serialize + std::fmt::Debug,
{
    SubscriptionMessage::from_json(data).map_err(|err| {
        tracing::trace!(
            ?data,
            %err,
            "failed to create `SubscriptionMessage` from json object"
        );

        ErrorObject::owned(8000, format!("serialization error: {err}"), None::<String>)
    })
}
