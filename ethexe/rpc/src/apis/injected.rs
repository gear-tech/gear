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

// +_+_+ return back implementation of Promises by hashes and injected API implementation in separate module (https://github.com/gear-tech/gear/commit/4138374dd246047187860adfe5f11e150e94b15d)

use crate::{RpcEvent, errors, metrics::InjectedApiMetrics};
use anyhow::Result;
use dashmap::DashMap;
use ethexe_common::{
    Address, HashOf,
    db::InjectedStorageRO,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance,
        SignedInjectedTransaction, SignedPromise,
    },
};
use ethexe_db::Database;
use futures::{StreamExt, stream::FuturesUnordered};
use jsonrpsee::{
    PendingSubscriptionSink, SubscriptionMessage, SubscriptionSink,
    core::{RpcResult, SubscriptionResult, async_trait},
    proc_macros::rpc,
    types::error::ErrorObjectOwned,
};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

const MAX_TRANSACTION_IDS: usize = 100;

#[cfg_attr(not(feature = "client"), rpc(server, namespace = "injected"))]
#[cfg_attr(feature = "client", rpc(server, client, namespace = "injected"))]
pub trait Injected {
    /// Just sends an injected transaction.
    #[method(name = "sendTransaction")]
    async fn send_transaction(
        &self,
        transaction: AddressedInjectedTransaction,
    ) -> RpcResult<InjectedTransactionAcceptance>;

    /// Sends an injected transaction and subscribes to its promise.
    #[subscription(
        name = "sendTransactionAndWatch",
        unsubscribe = "sendTransactionAndWatchUnsubscribe",
        item = SignedPromise
    )]
    async fn send_transaction_and_watch(
        &self,
        transaction: AddressedInjectedTransaction,
    ) -> SubscriptionResult;

    /// Retrieves injected transactions by the provided IDs
    #[method(name = "getTransactions")]
    async fn get_transactions(
        &self,
        transaction_ids: Vec<HashOf<InjectedTransaction>>,
    ) -> RpcResult<Vec<Option<SignedInjectedTransaction>>>;
}

type PromiseWaiters = Arc<DashMap<HashOf<InjectedTransaction>, oneshot::Sender<SignedPromise>>>;

/// Implementation of the injected transactions RPC API.
#[derive(Debug, Clone)]
pub struct InjectedApi {
    /// Node database instance.
    db: Database,
    /// Sender to forward RPC events to the main service.
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,
    /// Map of promise waiters.
    promise_waiters: PromiseWaiters,
    /// The metrics related to [`InjectedApi`]
    metrics: InjectedApiMetrics,
}

#[async_trait]
impl InjectedServer for InjectedApi {
    async fn send_transaction(
        &self,
        transaction: AddressedInjectedTransaction,
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
        transaction: AddressedInjectedTransaction,
    ) -> SubscriptionResult {
        let tx_hash = transaction.tx.data().to_hash();
        tracing::trace!(%tx_hash, "Called injected_subscribeTransactionPromise");

        // Check that the transaction wasn't already sent.
        if self.promise_waiters.get(&tx_hash).is_some() {
            tracing::warn!(tx_hash = ?tx_hash, "transaction was already sent");
            return Err(
                format!("transaction with the same hash was already sent: {tx_hash}").into(),
            );
        }

        // Register the promise waiter *before* the tx is broadcast.
        // The producer's MB execution can deliver `provide_promise`
        // back into this RPC server within microseconds (especially
        // when the producer happens to be the local node), and if
        // we register only after `forward_transaction` returns the
        // race window leaks promises into the "unregistered" warn
        // path. A `oneshot::Receiver` buffers the value, so even if
        // the promise lands before `pending.accept().await`
        // completes, `spawn_promise_waiter` still consumes it.
        let (promise_sender, promise_receiver) = oneshot::channel();
        self.promise_waiters.insert(tx_hash, promise_sender);

        if let Err(err) = self.forward_transaction(transaction).await {
            self.promise_waiters.remove(&tx_hash);
            return Err(err.into());
        }

        let subscription_sink = match pending.accept().await {
            Ok(sink) => sink,
            Err(err) => {
                tracing::warn!(
                    "failed to accept subscription for injected transaction promise: {err}"
                );
                self.promise_waiters.remove(&tx_hash);
                return Err(err.to_string().into());
            }
        };

        self.spawn_promise_waiter(subscription_sink, promise_receiver, tx_hash);

        Ok(())
    }

    async fn get_transactions(
        &self,
        transaction_ids: Vec<HashOf<InjectedTransaction>>,
    ) -> RpcResult<Vec<Option<SignedInjectedTransaction>>> {
        tracing::trace!(?transaction_ids, "Called injected_getTransactions");

        if transaction_ids.len() > MAX_TRANSACTION_IDS {
            return Err(errors::invalid_params(format!(
                "Too many transaction ids requested. Maximum is {MAX_TRANSACTION_IDS}.",
            )));
        }

        let transactions = transaction_ids
            .into_iter()
            .map(|tx_id| self.db.injected_transaction(tx_id))
            .collect::<Vec<Option<SignedInjectedTransaction>>>();

        Ok(transactions)
    }
}

impl InjectedApi {
    pub(crate) fn new(db: Database, rpc_sender: mpsc::UnboundedSender<RpcEvent>) -> Self {
        Self {
            db,
            rpc_sender,
            promise_waiters: PromiseWaiters::default(),
            metrics: InjectedApiMetrics::default(),
        }
    }

    pub fn send_promise(&self, promise: SignedPromise) {
        let Some((_, promise_sender)) = self.promise_waiters.remove(&promise.data().tx_hash) else {
            tracing::warn!(promise = ?promise, "receive unregistered promise");
            return;
        };

        self.metrics.injected_tx_active_subscriptions.decrement(1);

        match promise_sender.send(promise.clone()) {
            Ok(()) => {
                tracing::trace!(promise = ?promise, "sent promise to subscriber");
            }
            Err(promise) => tracing::trace!(promise = ?promise, "rpc promise receiver dropped"),
        }
    }

    /// Returns the number of current promise subscribers waiting for promises.
    #[cfg(test)]
    pub fn promise_subscribers_count(&self) -> usize {
        self.promise_waiters.len()
    }

    /// Forwards an injected transaction to the main service.
    ///
    /// Fans the transaction out across the current validator set: one
    /// `RpcEvent::InjectedTransaction` per validator, with that
    /// validator's address pinned as the `recipient`. Whichever
    /// validator the producer-side of BFT lands on next can pull the
    /// tx from its local mempool immediately, instead of waiting for
    /// the single RPC-receiving node to take its own producer turn.
    ///
    /// Returns the first `Accept` to come back, or the last `Reject`
    /// if every fan-out arm rejected. If the validator set isn't
    /// known yet (early boot, or `Database::memory()` in tests), we
    /// fall back to a single event with the original recipient — the
    /// caller's existing behavior is preserved.
    async fn forward_transaction(
        &self,
        transaction: AddressedInjectedTransaction,
    ) -> Result<InjectedTransactionAcceptance, ErrorObjectOwned> {
        let tx_hash = transaction.tx.data().to_hash();
        tracing::trace!(%tx_hash, ?transaction, "Called injected_sendTransaction with vars");

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

        let recipients: Vec<Address> = utils::current_validators(&self.db)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();

        if recipients.is_empty() {
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

            return response_receiver.await.map_err(|e| {
                tracing::error!(
                    "Response sender for the `RpcEvent::InjectedTransaction` was dropped: {e}"
                );
                errors::internal()
            });
        }

        let mut response_futures = FuturesUnordered::new();
        for recipient in recipients {
            let (response_sender, response_receiver) = oneshot::channel();
            let event = RpcEvent::InjectedTransaction {
                transaction: AddressedInjectedTransaction {
                    recipient,
                    tx: transaction.tx.clone(),
                },
                response_sender,
            };

            if let Err(err) = self.rpc_sender.send(event) {
                tracing::error!(
                    "Failed to send `RpcEvent::InjectedTransaction` event task: {err}. \
                    The receiving end in the main service might have been dropped."
                );
                return Err(errors::internal());
            }

            response_futures.push(response_receiver);
        }

        tracing::trace!(%tx_hash, "Broadcast transaction, waiting for first acceptance");

        let mut last_reject: Option<InjectedTransactionAcceptance> = None;
        while let Some(result) = response_futures.next().await {
            match result {
                Ok(InjectedTransactionAcceptance::Accept) => {
                    return Ok(InjectedTransactionAcceptance::Accept);
                }
                Ok(rejection) => last_reject = Some(rejection),
                Err(_) => {}
            }
        }

        last_reject.map(Ok).unwrap_or_else(|| {
            tracing::error!(
                %tx_hash,
                "All response senders for the `RpcEvent::InjectedTransaction` fan-out were dropped"
            );
            Err(errors::internal())
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
        let promise_waiters = self.promise_waiters.clone();
        self.metrics.injected_tx_active_subscriptions.increment(1);
        let metrics = self.metrics.clone();

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
                    metrics.injected_tx_active_subscriptions.decrement(1);
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
                );
            }
        });
    }
}

mod utils {
    use super::*;
    use anyhow::Context as _;
    use ethexe_common::{
        ValidatorsVec,
        db::{ConfigStorageRO, OnChainStorageRO},
    };
    use std::time::{Duration, SystemTime, SystemTimeError};

    /// Returns the validator set effective right now, used by the
    /// RPC layer to fan out an injected tx to every validator.
    /// Errors propagate when the protocol timelines aren't configured
    /// yet or when the era's validator vector is missing — callers
    /// fall back to single-recipient delivery in that case.
    pub fn current_validators(db: &Database) -> Result<ValidatorsVec> {
        let timelines = db.config().timelines;
        let now = now_since_unix_epoch()
            .context("system clock error")?
            .as_secs();
        let era = timelines
            .era_from_ts(now)
            .context("failed to calculate era from current timestamp")?;
        db.validators(era)
            .with_context(|| format!("validators not found for era={era}"))
    }

    /// Returns the current time since [SystemTime::UNIX_EPOCH].
    fn now_since_unix_epoch() -> Result<Duration, SystemTimeError> {
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
    }
}

#[cfg(test)]
mod tests {
    use super::{InjectedApi, InjectedServer, MAX_TRANSACTION_IDS};
    use ethexe_common::{
        db::InjectedStorageRW,
        ecdsa::PrivateKey,
        injected::{InjectedTransaction, SignedInjectedTransaction},
        mock::Mock,
    };
    use ethexe_db::Database;
    use tokio::sync::mpsc;

    fn make_signed_tx() -> SignedInjectedTransaction {
        SignedInjectedTransaction::create(PrivateKey::random(), InjectedTransaction::mock(()))
            .expect("creating signed injected transaction succeeds")
    }

    fn make_injected_api(db: Database) -> InjectedApi {
        let (sender, _receiver) = mpsc::unbounded_channel();
        InjectedApi::new(db, sender)
    }

    #[tokio::test]
    async fn test_get_transactions_found() {
        let db = Database::memory();
        let api = make_injected_api(db.clone());

        let tx = make_signed_tx();
        let tx_hash = tx.data().to_hash();
        db.set_injected_transaction(tx.clone());

        let result = api.get_transactions(vec![tx_hash]).await.unwrap();
        assert_eq!(result, vec![Some(tx)]);
    }

    #[tokio::test]
    async fn test_get_transactions_not_found() {
        let db = Database::memory();
        let api = make_injected_api(db.clone());

        let tx_hash = make_signed_tx().data().to_hash();
        // Transaction not stored in DB.
        let result = api.get_transactions(vec![tx_hash]).await.unwrap();
        assert_eq!(result, vec![None]);
    }

    #[tokio::test]
    async fn test_get_transactions_mixed() {
        let db = Database::memory();
        let api = make_injected_api(db.clone());

        let tx1 = make_signed_tx();
        let tx2 = make_signed_tx();
        let hash1 = tx1.data().to_hash();
        let hash2 = tx2.data().to_hash();
        db.set_injected_transaction(tx1.clone());
        // tx2 not stored.

        let result = api.get_transactions(vec![hash1, hash2]).await.unwrap();
        assert_eq!(result, vec![Some(tx1), None]);
    }

    #[tokio::test]
    async fn test_get_transactions_empty() {
        let db = Database::memory();
        let api = make_injected_api(db.clone());

        let result = api.get_transactions(vec![]).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_get_transactions_exceeds_limit() {
        let db = Database::memory();
        let api = make_injected_api(db.clone());

        let ids = (0..=MAX_TRANSACTION_IDS)
            .map(|_| make_signed_tx().data().to_hash())
            .collect();

        let result = api.get_transactions(ids).await;
        assert!(result.is_err());
    }
}
