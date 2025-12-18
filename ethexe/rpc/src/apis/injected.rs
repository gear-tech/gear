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
use futures::{FutureExt, Stream, StreamExt};
use jsonrpsee::{
    PendingSubscriptionSink, SubscriptionMessage,
    core::{RpcResult, SubscriptionResult, async_trait},
    proc_macros::rpc,
    types::{ErrorObject, ErrorObjectOwned},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::wrappers::BroadcastStream;

const MAX_PROMISE_CHANNEL_CAPACITY: usize = 1024;

/// The timeout for receiving the promise for an injected transaction.
/// Normally, the promise must be received within a few slots after the transactions submission.
const PROMISE_RECEIVING_TIMEOUT_SECS: u64 = alloy::eips::merge::SLOT_DURATION_SECS * 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InjectedTransactionAcceptance {
    Accept,
    Reject { reason: String },
}

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

/// Implementation of the Injected RPC API.
#[derive(derive_more::Debug, Clone)]
pub struct InjectedApi {
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,

    #[debug(skip)]
    promise_manager: PromiseManager,
}

/// [`PromiseManager`] is responsible for delivering signed promises
/// to waiters for exact injected tx promise and broadcasting all
/// incoming promises to subscribers.
#[derive(Clone)]
pub struct PromiseManager {
    /// The waiters for exact injected tx promises.
    promise_waiters: Arc<DashMap<HashOf<InjectedTransaction>, oneshot::Sender<SignedPromise>>>,

    /// The broadcaster for promise subscribers ([`InjectedServer::subscribe_promises`]).
    promise_broadcaster: broadcast::Sender<SignedPromise>,
}

impl PromiseManager {
    /// Creates a new instance of [`PromiseManager`].
    pub(crate) fn new() -> Self {
        let promise_broadcaster = broadcast::Sender::new(MAX_PROMISE_CHANNEL_CAPACITY);
        Self {
            promise_waiters: Arc::new(DashMap::new()),
            promise_broadcaster,
        }
    }

    /// Creates a new stream of signed promises for promises subscription.
    pub(crate) fn new_promise_stream(&self) -> BroadcastStream<SignedPromise> {
        BroadcastStream::new(self.promise_broadcaster.subscribe())
    }

    /// Handles an incoming signed promise.
    /// 1. If there is a waiter for the promise's transaction hash, sends the promise to it.
    /// 2. Broadcasts the promise to all subscribers.
    pub(crate) fn handle_promise(&self, promise: SignedPromise) {
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
        // TODO kuzmindev: remove clone here
        match self.promise_broadcaster.send(promise.clone()) {
            Ok(receivers_count) => {
                tracing::trace!(
                    ?promise,
                    "promise broadcasted to {receivers_count} subscribers"
                );
            }
            Err(err) => {
                tracing::trace!(
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

    fn register_waiter_inner(&self) {}
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
}

#[async_trait]
impl InjectedServer for InjectedApi {
    async fn send_transaction(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> RpcResult<InjectedTransactionAcceptance> {
        let tx_hash = transaction.tx.data().to_hash();
        tracing::trace!(%tx_hash, ?transaction, "Called injected_sendTransaction with vars");

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

        tracing::trace!(%tx_hash, "Accept transition, waiting for promise");

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

    async fn send_transaction_and_watch(
        &self,
        pending: PendingSubscriptionSink,
        transaction: RpcOrNetworkInjectedTx,
    ) -> SubscriptionResult {
        let tx_hash = transaction.tx.data().to_hash();
        tracing::trace!(%tx_hash, ?transaction, "Called injected_sendTransactionAndWatch");

        // Check if a waiter for the transaction hash already exists.
        if self.promise_manager.waiter_exists_for(&tx_hash) {
            tracing::trace!(tx_hash = ?tx_hash, "rejecting subscription: subscriber already exists");
            let err = SubscriberAlreadyExistsError(tx_hash);
            pending.reject(errors::bad_request(err.to_string())).await;
            return Ok(());
        }

        let (response_sender, response_receiver) = oneshot::channel();

        if let Err(err) = self.rpc_sender.send(RpcEvent::InjectedTransaction {
            transaction,
            response_sender,
        }) {
            tracing::error!(
                "Failed to send `RpcEvent::InjectedTransaction` event task: {err}. \
                The receiving end in the main service might have been dropped."
            );
            return Err(errors::internal().into());
        }

        if let InjectedTransactionAcceptance::Reject { reason } = response_receiver.await? {
            tracing::trace!(
                tx_hash = ?tx_hash,
                reject_reason = ?reason,
                "reject injected transaction"
            );
            pending.reject(errors::bad_request(reason)).await;
            return Ok(());
        }

        let subscription_sink = match pending.accept().await {
            Ok(sink) => sink,
            Err(err) => {
                tracing::warn!(
                    "failed to accept subscription for injected transaction promise: {err}"
                );
                return Ok(());
            }
        };

        tracing::trace!(?tx_hash, "Accept transaction, start promise waiter");

        // TODO kuzmindev: i am not sure about concurrency safety here.
        // Safe because we in a few lines above we checked that no existing waiter exists for the tx_hash.
        let promise_receiver = unsafe { self.promise_manager.register_waiter_unchecked(tx_hash) };
        let promise_manager = self.promise_manager.clone();

        tokio::spawn(async move {
            let promise_future = tokio::time::timeout(
                tokio::time::Duration::from_secs(PROMISE_RECEIVING_TIMEOUT_SECS),
                promise_receiver,
            );

            let result = promise_future.await;
            promise_manager.remove_waiter(&tx_hash);

            let promise = match result {
                Ok(Ok(promise)) => promise,
                Ok(Err(err)) => {
                    tracing::trace!("");
                    return;
                }
                Err(err) => {
                    tracing::trace!("");
                    return;
                }
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

            if let Err(err) = subscription_sink.send(promise_msg).await {
                tracing::warn!(
                    tx_hash = ?tx_hash,
                    error = %err,
                    "failed to send subscription message"
                )
            }
        });

        Ok(())
    }

    async fn subscribe_promises(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
        tracing::trace!("Called injected_subscribePromises");
        let sink = pending.accept().await?;
        let mut stream = self.promise_manager.new_promise_stream();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = sink.closed() => {
                        tracing::trace!("Promise subscription sink closed");
                        break
                    }
                    promise = stream.next() => {
                        let Some(_promise) = promise else {
                            tracing::trace!("Promise stream ended");
                            break
                        };

                        todo!("Handle promise result");
                    }
                }
            }
        });

        Ok(())
    }
}

/// Error returned when a subscriber for an injected transaction already exists.
#[derive(thiserror::Error, Debug, Clone)]
#[error("Subscriber for injected transaction with hash {0} already exists")]
pub struct SubscriberAlreadyExistsError(HashOf<InjectedTransaction>);
