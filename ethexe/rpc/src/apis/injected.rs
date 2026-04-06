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

use crate::{RpcEvent, errors, metrics::InjectedApiMetrics};
use anyhow::Result;
use dashmap::DashMap;
use ethexe_common::{
    Address, HashOf,
    consensus::block_producer_for,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance,
        SignedPromise,
    },
};
use ethexe_db::Database;
use jsonrpsee::{
    PendingSubscriptionSink, SubscriptionMessage, SubscriptionSink,
    core::{RpcResult, SubscriptionResult, async_trait},
    proc_macros::rpc,
    types::error::ErrorObjectOwned,
};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

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
        self.metrics.send_and_watch_injected_tx_calls.increment(1);

        // Check, that transaction wasn't already send.
        if self.promise_waiters.get(&tx_hash).is_some() {
            tracing::warn!(tx_hash = ?tx_hash, "transaction was already sent");
            return Err(
                format!("transaction with the same hash was already sent: {tx_hash}").into(),
            );
        }

        let _acceptance = self.forward_transaction(transaction).await?;

        // Try accept subscription, if some errors occur, just log them and return error to client.
        let subscription_sink = pending.accept().await.inspect_err(|err| {
            tracing::warn!("failed to accept subscription for injected transaction promise: {err}");
        })?;

        let (promise_sender, promise_receiver) = oneshot::channel();
        self.promise_waiters.insert(tx_hash, promise_sender);
        self.spawn_promise_waiter(subscription_sink, promise_receiver, tx_hash);

        Ok(())
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
                self.metrics.injected_tx_promises_given.increment(1);
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

    /// This function forwards [`AddressedInjectedTransaction`] to main service and waits for its acceptance.
    async fn forward_transaction(
        &self,
        mut transaction: AddressedInjectedTransaction,
    ) -> Result<InjectedTransactionAcceptance, ErrorObjectOwned> {
        let tx_hash = transaction.tx.data().to_hash();
        tracing::trace!(%tx_hash, ?transaction, "Called injected_sendTransaction with vars");
        self.metrics.send_injected_tx_calls.increment(1);

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

        if transaction.recipient == Address::default() {
            utils::route_transaction(&self.db, &mut transaction)?;
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
    use ethexe_common::{
        Address,
        db::{ConfigStorageRO, OnChainStorageRO},
    };
    use std::time::{Duration, SystemTime, SystemTimeError};
    use tracing::{error, trace};

    pub(super) const NEXT_PRODUCER_THRESHOLD_MS: u128 = 50;

    pub fn route_transaction(
        db: &Database,
        tx: &mut AddressedInjectedTransaction,
    ) -> RpcResult<()> {
        let now = now_since_unix_epoch().map_err(|err| {
            error!("system clock error: {err}");
            crate::errors::internal()
        })?;

        let next_producer = calculate_next_producer(db, now).map_err(|err| {
            trace!("calculate next producer error: {err}");
            crate::errors::internal()
        })?;
        tx.recipient = next_producer;

        Ok(())
    }

    /// Calculates the producer address to route an injected transaction to.
    pub(super) fn calculate_next_producer(db: &Database, now: Duration) -> Result<Address> {
        let timelines = db.config().timelines;

        // Compute the remaining time in the current slot.
        // If the slot is close to ending transaction will be sent to next-next producer.
        // That avoids sending the transaction to a validator that probably will not receive it in time.
        let slot_ms = Duration::from_secs(timelines.slot).as_millis();
        let remaining_time = slot_ms - (now.as_millis() % slot_ms);

        let target_timestamp = match remaining_time > NEXT_PRODUCER_THRESHOLD_MS {
            true => now.as_secs() + timelines.slot,
            false => now.as_secs() + 2 * timelines.slot,
        };

        let era = timelines.era_from_ts(target_timestamp);

        let validators = db
            .validators(era)
            .ok_or_else(|| anyhow::anyhow!("validators not found for era={era}"))?;

        Ok(block_producer_for(
            &validators,
            target_timestamp,
            timelines.slot,
        ))
    }

    /// Returns the current time since [SystemTime::UNIX_EPOCH].
    fn now_since_unix_epoch() -> Result<Duration, SystemTimeError> {
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
    }
}

#[cfg(test)]
mod tests {
    use super::utils;
    use ethexe_common::{
        Address, ProtocolTimelines, ValidatorsVec,
        db::{ConfigStorageRO, OnChainStorageRW, SetConfig},
    };
    use ethexe_db::Database;
    use gear_core::pages::num_traits::ToPrimitive;
    use std::{ops::Sub, time::Duration};

    const SLOT: u64 = 10;
    const ERA: u64 = 1000;

    fn setup_db(db: &Database) -> ValidatorsVec {
        let validators = ValidatorsVec::from_iter((0..10u64).map(|i| Address::from(i)));

        let timelines = ProtocolTimelines {
            slot: SLOT,
            era: ERA,
            ..Default::default()
        };
        db.set_validators(0, validators.clone());
        let mut config = db.config().clone();
        config.timelines = timelines;
        db.set_config(config);
        validators
    }

    #[test]
    fn test_calculate_next_producer_return_next() {
        let db = Database::memory();
        let validators = setup_db(&db);

        let now = Duration::from_secs(SLOT / 2);
        let producer = utils::calculate_next_producer(&db, now).unwrap();

        assert_eq!(validators[1], producer);
    }

    #[test]
    fn test_calculate_next_producer_return_next_next() {
        let db = Database::memory();
        let validators = setup_db(&db);

        let half_threshold = utils::NEXT_PRODUCER_THRESHOLD_MS.to_u64().unwrap();
        let now = Duration::from_secs(SLOT).sub(Duration::from_millis(half_threshold));
        let producer = utils::calculate_next_producer(&db, now).unwrap();

        assert_eq!(validators[2], producer);
    }

    #[test]
    fn test_calculate_next_producer_in_next_era() {
        let db = Database::memory();
        let validators = setup_db(&db);

        // Prepate next era validators
        let mut next_era_validators = validators.clone();
        next_era_validators[0] = validators[9];
        db.set_validators(1, next_era_validators.clone());

        let now = Duration::from_secs(ERA).sub(Duration::from_secs(1));
        let producer = utils::calculate_next_producer(&db, now).unwrap();

        assert_eq!(next_era_validators[0], producer);
    }
}
