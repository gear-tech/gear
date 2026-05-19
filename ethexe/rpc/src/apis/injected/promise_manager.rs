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
    injected::{InjectedTransaction, Promise, SignedCompactPromise, SignedPromise},
};
use ethexe_db::Database;
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{trace, warn};

// TODO: Issues #5384 and #5385.
type PromiseSubscribers = Arc<DashMap<HashOf<InjectedTransaction>, oneshot::Sender<SignedPromise>>>;
type PromisesComputationWaiting = Arc<DashMap<HashOf<InjectedTransaction>, SignedCompactPromise>>;

/// The manager for promise subscribers.
#[derive(Debug, Clone)]
pub struct PromiseSubscriptionManager {
    db: Database,
    subscribers: PromiseSubscribers,

    waiting_for_compute: PromisesComputationWaiting,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RegisterSubscriberError {
    #[error("Subscriber for this transaction already exists, tx_hash={0}")]
    AlreadyRegistered(HashOf<InjectedTransaction>),
    #[error("Promise for this transaction is already resolved, tx_hash={0}")]
    AlreadyResolved(HashOf<InjectedTransaction>, SignedPromise),
}

type TimeoutReceiver = tokio::time::Timeout<oneshot::Receiver<SignedPromise>>;

/// The pending [SignedPromise] subscriber.
/// Subscriber will be spawned in separate tokio runtime task and will wait for promise.
///
/// Important: to avoid infinite waiting we wrap [oneshot::Receiver] into [tokio::time::timeout].
pub struct PendingSubscriber {
    /// Tx hash waiting promise for.
    tx_hash: HashOf<InjectedTransaction>,
    /// Wrapped promise [oneshot::Receiver].
    receiver: TimeoutReceiver,
}

impl PendingSubscriber {
    pub fn new(
        db: &Database,
        tx_hash: HashOf<InjectedTransaction>,
        receiver: oneshot::Receiver<SignedPromise>,
    ) -> Self {
        let timeout_duration = utils::promise_waiting_timeout(db);
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
            waiting_for_compute: PromisesComputationWaiting::default(),
        }
    }

    // TODO: Issue #5402
    pub fn try_register_subscriber(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> Result<PendingSubscriber, RegisterSubscriberError> {
        if let Some(promise) = self.db.promise(tx_hash)
            && let Some(compact) = self.db.compact_promise(tx_hash)
            && let Ok(signed_promise) = compact.restore(promise)
        {
            return Err(RegisterSubscriberError::AlreadyResolved(tx_hash, signed_promise));
        }

        match self.subscribers.entry(tx_hash) {
            Entry::Occupied(_) => Err(RegisterSubscriberError::AlreadyRegistered(tx_hash)),
            Entry::Vacant(entry) => {
                let (sender, receiver) = oneshot::channel();
                entry.insert(sender);

                if let Some(promise) = self.db.promise(tx_hash)
                    && let Some(compact) = self.db.compact_promise(tx_hash)
                    && let Ok(signed_promise) = compact.restore(promise)
                {
                    self.dispatch_promise(signed_promise.clone());
                    return Err(RegisterSubscriberError::AlreadyResolved(tx_hash, signed_promise));
                }

                Ok(PendingSubscriber::new(&self.db, tx_hash, receiver))
            }
        }
    }

    pub fn cancel_registration(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> Option<oneshot::Sender<SignedPromise>> {
        self.subscribers.remove(&tx_hash).map(|(_, v)| v)
    }

    // TODO: Issue #5403
    pub fn on_compact_promise(&self, compact: SignedCompactPromise) {
        trace!(?compact, "received new compact promise");
        let tx_hash = compact.data().tx_hash;

        match self.db.promise(tx_hash) {
            Some(promise) => match compact.restore(promise) {
                Ok(signed_promise) => {
                    self.db.set_compact_promise(&compact);
                    self.dispatch_promise(signed_promise);
                }

                Err(err) => {
                    warn!(
                        ?compact, %tx_hash, error=?err, "failed to create signed promise from parts, producer send invalid signature: compact_promise={compact:?}"
                    );
                    self.waiting_for_compute.insert(tx_hash, compact);
                }
            },
            None => {
                trace!("not found promise in database, waiting for computation...");
                self.waiting_for_compute.insert(tx_hash, compact);
            }
        }
    }

    pub fn on_computed_promise(&self, promise: Promise) {
        trace!(?promise, "received new computed promise");
        self.db.set_promise(&promise);

        if let Some((_, compact_promise)) = self.waiting_for_compute.remove(&promise.tx_hash) {
            match compact_promise.restore(promise) {
                Ok(signed_promise) => {
                    self.db.set_compact_promise(&compact_promise);
                    self.dispatch_promise(signed_promise);
                }
                Err(_err) => {
                    trace!(?compact_promise, tx_hash=?compact_promise.data().tx_hash, "failed to create signed promise from parts");
                }
            }
        }
    }

    fn dispatch_promise(&self, promise: SignedPromise) {
        if let Some((_, sender)) = self.subscribers.remove(&promise.data().tx_hash)
            && let Err(unsent_promise) = sender.send(promise)
        {
            trace!("failed to send promise to subscriber, promise={unsent_promise:?}");
        }
    }

    #[cfg(test)]
    pub fn subscribers_count(&self) -> usize {
        self.subscribers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{PrivateKey, db::InjectedStorageRW, mock::Mock};

    fn make_signed_promise() -> SignedPromise {
        SignedPromise::create(PrivateKey::random(), Promise::mock(())).expect("signed promise")
    }

    #[test]
    fn register_subscriber_returns_already_resolved_for_available_promise() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());

        let signed_promise = make_signed_promise();
        let tx_hash = signed_promise.data().tx_hash;
        db.set_promise(signed_promise.data());
        db.set_compact_promise(signed_promise.compact());

        match manager.try_register_subscriber(tx_hash) {
            Err(RegisterSubscriberError::AlreadyResolved(actual_tx_hash, actual_promise)) => {
                assert_eq!(actual_tx_hash, tx_hash);
                assert_eq!(actual_promise, signed_promise);
            }
            res => panic!("unexpected registration result: {res:?}"),
        }

        assert_eq!(manager.subscribers_count(), 0);
    }
}

mod utils {
    use ethexe_common::db::ConfigStorageRO;

    /// The maximum number of slots RPC will wait for transaction promise.
    const MAX_PROMISE_WAITING_SLOTS: u64 = 20;

    /// Returns the maximum time that spawned [super::PendingSubscriber] will wait for promise.
    pub fn promise_waiting_timeout<DB: ConfigStorageRO>(db: &DB) -> std::time::Duration {
        let slot_duration_secs = db.config().timelines.slot.get();
        std::time::Duration::from_secs(slot_duration_secs * MAX_PROMISE_WAITING_SLOTS)
    }
}
