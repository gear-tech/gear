// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use anyhow::Result;
use dashmap::{DashMap, mapref::entry::Entry};
use ethexe_common::{
    HashOf,
    db::{InjectedStorageRO, InjectedStorageRW},
    injected::{
        InjectedTransaction, Promise, SignedCompactTxReceipt, SignedTxReceipt,
        TryFillPromiseResult, UnfilledPromiseReceipt, UpgradedReceipt,
    },
};
use ethexe_db::Database;
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{trace, warn};

// TODO: Issues #5384 and #5385.
type PromiseSubscribers =
    Arc<DashMap<HashOf<InjectedTransaction>, oneshot::Sender<SignedTxReceipt>>>;
type ReceiptsComputationWaiting = Arc<DashMap<HashOf<InjectedTransaction>, UnfilledPromiseReceipt>>;

/// The manager for promise subscribers.
#[derive(Debug, Clone)]
pub struct PromiseSubscriptionManager {
    db: Database,
    subscribers: PromiseSubscribers,

    waiting_for_compute: ReceiptsComputationWaiting,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RegisterSubscriberError {
    #[error("Subscriber for this transaction already exists, tx_hash={0}")]
    AlreadyRegistered(HashOf<InjectedTransaction>),
}

type TimeoutReceiver = tokio::time::Timeout<oneshot::Receiver<SignedTxReceipt>>;

/// The pending [SignedTxReceipt] subscriber.
/// Subscriber will be spawned in separate tokio runtime task and will wait for promise.
///
/// Important: to avoid infinite waiting we wrap [oneshot::Receiver] into [tokio::time::timeout].
pub struct PendingSubscriber {
    /// Tx hash waiting promise for.
    tx_hash: HashOf<InjectedTransaction>,
    /// Wrapped tx receipt [oneshot::Receiver].
    receiver: TimeoutReceiver,
}

impl PendingSubscriber {
    pub fn new(
        db: &Database,
        tx_hash: HashOf<InjectedTransaction>,
        receiver: oneshot::Receiver<SignedTxReceipt>,
    ) -> Self {
        let timeout_duration = utils::receipt_waiting_timeout(db);
        let receiver = tokio::time::timeout(timeout_duration, receiver);
        Self { tx_hash, receiver }
    }

    pub fn into_parts(self) -> (HashOf<InjectedTransaction>, TimeoutReceiver) {
        (self.tx_hash, self.receiver)
    }
}

impl PromiseSubscriptionManager {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            subscribers: PromiseSubscribers::default(),
            waiting_for_compute: ReceiptsComputationWaiting::default(),
        }
    }

    // TODO: Issue #5402
    pub fn try_register_subscriber(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> Result<PendingSubscriber, RegisterSubscriberError> {
        match self.subscribers.entry(tx_hash) {
            Entry::Occupied(_) => Err(RegisterSubscriberError::AlreadyRegistered(tx_hash)),
            Entry::Vacant(entry) => {
                let (sender, receiver) = oneshot::channel();
                entry.insert(sender);
                Ok(PendingSubscriber::new(&self.db, tx_hash, receiver))
            }
        }
    }

    pub fn cancel_registration(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> Option<oneshot::Sender<SignedTxReceipt>> {
        self.subscribers.remove(&tx_hash).map(|(_, v)| v)
    }

    // TODO: Issue #5403
    pub fn on_tx_receipt(&self, receipt: SignedCompactTxReceipt) {
        trace!(?receipt, "received new compact receipt");

        let unfilled_promise = match receipt.upgrade() {
            UpgradedReceipt::Ready(receipt) => {
                self.store_and_dispatch_receipt(receipt);
                return;
            }
            UpgradedReceipt::Pending(unfilled_promise) => unfilled_promise,
        };

        let tx_hash = unfilled_promise.tx_hash;
        match self.db.promise(tx_hash) {
            Some(promise) => match unfilled_promise.try_fill_with(promise) {
                TryFillPromiseResult::Filled(receipt) => self.store_and_dispatch_receipt(receipt),
                TryFillPromiseResult::HashesMismatch(unfilled) => {
                    warn!(
                        ?unfilled,
                        "locally computed promise do not match producer's receipt"
                    );
                    self.waiting_for_compute.insert(tx_hash, unfilled);
                }
            },
            None => {
                trace!("not found promise in database, waiting for computation...");
                self.waiting_for_compute.insert(tx_hash, unfilled_promise);
            }
        }
    }

    pub fn on_computed_promise(&self, promise: Promise) {
        trace!(?promise, "received new computed promise");
        self.db.set_promise(&promise);

        let Some((_, unfilled_promise)) = self.waiting_for_compute.remove(&promise.tx_hash) else {
            return;
        };

        match unfilled_promise.try_fill_with(promise) {
            TryFillPromiseResult::Filled(signed_receipt) => {
                self.store_and_dispatch_receipt(signed_receipt)
            }
            TryFillPromiseResult::HashesMismatch(unfilled) => {
                warn!(
                    ?unfilled,
                    "locally computed promise do not match producer's receipt"
                );
                self.waiting_for_compute.insert(unfilled.tx_hash, unfilled);
            }
        }
    }

    fn dispatch_receipt(&self, receipt: SignedTxReceipt) {
        if let Some((_, sender)) = self.subscribers.remove(&receipt.data().tx_hash())
            && let Err(unsent_receipt) = sender.send(receipt)
        {
            trace!("failed to send receipt to subscriber, receipt={unsent_receipt:?}");
        }
    }

    fn store_and_dispatch_receipt(&self, receipt: SignedTxReceipt) {
        self.db.set_receipt(&receipt);
        self.dispatch_receipt(receipt);
    }

    #[cfg(test)]
    pub fn subscribers_count(&self) -> usize {
        self.subscribers.len()
    }
}

mod utils {
    use ethexe_common::db::ConfigStorageRO;

    /// The maximum number of slots RPC will wait for transaction promise.
    const MAX_PROMISE_WAITING_SLOTS: u64 = 20;

    /// Returns the maximum time that spawned [super::PendingSubscriber] will wait for promise.
    pub fn receipt_waiting_timeout<DB: ConfigStorageRO>(db: &DB) -> std::time::Duration {
        let slot_duration_secs = db.config().timelines.slot.get();
        std::time::Duration::from_secs(slot_duration_secs * MAX_PROMISE_WAITING_SLOTS)
    }
}
