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

use crate::{
    RpcEvent, errors,
    metrics::{InjectedApiMetrics, RpcApiMetrics},
};
use dashmap::DashMap;
use ethexe_common::{
    HashOf,
    injected::{InjectedTransaction, RpcOrNetworkInjectedTx, SignedPromise},
};
use jsonrpsee::{
    PendingSubscriptionSink, SubscriptionMessage,
    core::{RpcResult, SubscriptionResult, async_trait},
    proc_macros::rpc,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InjectedTransactionAcceptance {
    Accept,
    Reject { reason: String },
}

#[cfg_attr(not(feature = "test-utils"), rpc(server))]
#[cfg_attr(feature = "test-utils", rpc(server, client))]
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
}

#[derive(Debug, Clone)]
pub struct InjectedApi {
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,
    promise_waiters: Arc<DashMap<HashOf<InjectedTransaction>, oneshot::Sender<SignedPromise>>>,
    metrics: InjectedApiMetrics,
}

impl InjectedApi {
    pub(crate) fn new(
        rpc_sender: mpsc::UnboundedSender<RpcEvent>,
        metrics: InjectedApiMetrics,
    ) -> Self {
        Self {
            rpc_sender,
            promise_waiters: Arc::new(DashMap::new()),
            metrics,
        }
    }
}

impl InjectedApi {
    pub fn send_promise(&self, promise: SignedPromise) {
        let Some((_, promise_sender)) = self.promise_waiters.remove(&promise.data().tx_hash) else {
            tracing::warn!(promise = ?promise, "receive unregistered promise");
            return;
        };

        if let Err(promise) = promise_sender.send(promise) {
            tracing::trace!(promise = ?promise, "rpc promise receiver dropped");
        }
    }
}

#[async_trait]
impl InjectedServer for InjectedApi {
    async fn send_transaction(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> RpcResult<InjectedTransactionAcceptance> {
        tracing::trace!("Called injected_sendTransaction with vars: {transaction:?}");

        let (response_sender, response_receiver) = oneshot::channel();
        let event = RpcEvent::InjectedTransaction {
            transaction,
            response_sender,
        };

        if let Err(err) = self.rpc_sender.send(event) {
            log::error!(
                "Failed to send `RpcEvent::InjectedTransaction` event task: {err}. \
                The receiving end in the main service might have been dropped."
            );
            return Err(errors::internal());
        }

        response_receiver.await.map_err(|e| {
            // No panic case, as a responsibility of the RPC API is fulfilled.
            // The dropped sender signalizes that the main service has crashed
            // or is malformed, so problems should be handled there.
            log::error!("Response sender for the `RpcEvent::InjectedTransaction` was dropped: {e}");
            errors::internal()
        })
    }

    async fn send_transaction_and_watch(
        &self,
        pending: PendingSubscriptionSink,
        transaction: RpcOrNetworkInjectedTx,
    ) -> SubscriptionResult {
        let tx_hash = transaction.tx.data().to_hash();

        // Checks, that transaction wasn't already send.
        if self.promise_waiters.get(&tx_hash).is_some() {
            tracing::warn!(tx_hash = ?tx_hash, "transaction was already sent");
            return Err(
                format!("transaction with the same hash was already sent: {tx_hash}").into(),
            );
        }

        let (response_sender, response_receiver) = oneshot::channel();
        let (promise_sender, promise_receiver) = oneshot::channel();

        let event = RpcEvent::InjectedTransaction {
            transaction,
            response_sender,
        };

        if let Err(err) = self.rpc_sender.send(event) {
            log::error!(
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

        self.promise_waiters.insert(tx_hash, promise_sender);

        tokio::spawn(async move {
            let Ok(promise) = promise_receiver.await else {
                return;
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
}
