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
use ethexe_common::{
    HashOf,
    injected::{InjectedTransaction, Promise, RpcOrNetworkInjectedTx},
};
use jsonrpsee::{
    DisconnectError, PendingSubscriptionSink, SubscriptionMessage,
    core::{RpcResult, SubscriptionResult, async_trait},
    proc_macros::rpc,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, mpsc::UnboundedSender, oneshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InjectedTransactionAcceptance {
    Accept,
    #[allow(unused)]
    Reject,
}

#[cfg_attr(not(feature = "test-utils"), rpc(server))]
#[cfg_attr(feature = "test-utils", rpc(server, client))]
pub trait Injected {
    #[method(name = "injected_sendTransaction")]
    async fn send_transaction(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> RpcResult<InjectedTransactionAcceptance>;

    #[subscription(name = "subscribe_transactionPromise", unsubscribe = "unsubscribe_transactionPromise", item = Promise)]
    async fn subscribe_promise(&self, transaction: RpcOrNetworkInjectedTx) -> SubscriptionResult;
}

type PromiseWaitersMap = HashMap<HashOf<InjectedTransaction>, oneshot::Sender<Promise>>;

#[derive(Debug, Clone)]
pub struct InjectedApi {
    rpc_sender: UnboundedSender<RpcEvent>,
    promise_waiters: Arc<Mutex<PromiseWaitersMap>>,
}

impl InjectedApi {
    pub(crate) fn new(rpc_sender: UnboundedSender<RpcEvent>) -> Self {
        Self {
            rpc_sender,
            promise_waiters: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl InjectedApi {
    pub async fn receive_promise(&self, promise: Promise) {
        let mut guard = self.promise_waiters.lock().await;

        let Some(promise_sender) = guard.remove(&promise.tx_hash) else {
            tracing::trace!(promise = ?promise, "receive unregistered promise");
            return;
        };

        if let Err(_p) = promise_sender.send(promise) {
            tracing::trace!("rpc promise receiver dropped");
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
        self.rpc_sender
            .send(RpcEvent::InjectedTransaction {
                transaction,
                response_sender,
            })
            .map_err(|e| {
                // That could be a panic case, as rpc_receiver must not be dropped,
                // but the main service works independently from rpc and can be malformed.
                log::error!(
                    "Failed to send `RpcEvent::InjectedTransaction` event task: {e}. \
                    The receiving end in the main service might have been dropped."
                );
                errors::internal()
            })?;

        response_receiver.await.map_err(|e| {
            // No panic case, as a responsibility of the RPC API is fulfilled.
            // The dropped sender signalizes that the main service has crashed
            // or is malformed, so problems should be handled there.
            log::error!("Response sender for the `RpcEvent::InjectedTransaction` was dropped: {e}");
            errors::internal()
        })
    }

    async fn subscribe_promise(
        &self,
        pending: PendingSubscriptionSink,
        transaction: RpcOrNetworkInjectedTx,
    ) -> SubscriptionResult {
        let (response_sender, response_receiver) = oneshot::channel();
        let (promise_sender, promise_receiver) = oneshot::channel();

        let tx_hash = transaction.tx.data().to_hash();

        self.rpc_sender
            .send(RpcEvent::InjectedTransaction {
                transaction,
                response_sender,
            })
            .map_err(|e| {
                // That could be a panic case, as rpc_receiver must not be dropped,
                // but the main service works independently from rpc and can be malformed.
                log::error!(
                    "Failed to send `RpcEvent::InjectedTransaction` event task: {e}. \
                    The receiving end in the main service might have been dropped."
                );
                errors::internal()
            })?;

        let _accepted = response_receiver.await?;
        let sink = pending.accept().await?;

        let mut guard = self.promise_waiters.lock().await;
        guard.insert(tx_hash, promise_sender);

        tokio::spawn(async move {
            let promise = match promise_receiver.await {
                Ok(promise) => promise,
                Err(_err) => {
                    return;
                }
            };

            let json = match serde_json::value::to_raw_value(&promise) {
                Ok(raw_json) => raw_json,
                Err(_err) => {
                    return;
                }
            };

            let msg = SubscriptionMessage::from_json(&json).expect("correct message");

            if let Err(DisconnectError(_msg)) = sink.send(msg).await {
                tracing::trace!(
                    "failed to send promise for subscription, because of receiver disconnect"
                )
            }
        });

        Ok(())
    }
}
