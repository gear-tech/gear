// This file is part of Gear.
//
// Copyright (C) 2025-2026 Gear Technologies Inc.
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

//! Subscription registry for injected-transaction promises.
//!
//! [`PromiseSubscriptionManager`] holds a `tx_hash -> oneshot::Sender`
//! map that the RPC server uses to deliver promises produced by the
//! local node back to the client awaiting them.

use dashmap::{DashMap, mapref::entry::Entry};
use ethexe_common::{
    HashOf,
    db::ConfigStorageRO,
    injected::{InjectedTransaction, SignedPromise},
};
use ethexe_db::Database;
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{trace, warn};

type PromiseSubscribers = Arc<DashMap<HashOf<InjectedTransaction>, oneshot::Sender<SignedPromise>>>;

/// Maximum number of slots an RPC client will wait for an injected-tx
/// promise before the spawned subscriber is dropped.
const MAX_PROMISE_WAITING_SLOTS: u64 = 20;

#[derive(Debug, Clone)]
pub enum RegisterSubscriberError {
    AlreadyRegistered(HashOf<InjectedTransaction>),
}

impl std::fmt::Display for RegisterSubscriberError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRegistered(tx_hash) => write!(
                f,
                "Subscriber for this transaction already exists, tx_hash={tx_hash}"
            ),
        }
    }
}

impl std::error::Error for RegisterSubscriberError {}

type TimeoutReceiver = tokio::time::Timeout<oneshot::Receiver<SignedPromise>>;

/// A pending [`SignedPromise`] subscriber.
///
/// Wraps the underlying `oneshot::Receiver` in a `tokio::time::timeout`
/// so the subscriber can't dangle forever if no promise ever arrives.
pub struct PendingSubscriber {
    tx_hash: HashOf<InjectedTransaction>,
    receiver: TimeoutReceiver,
}

impl PendingSubscriber {
    fn new(
        db: &Database,
        tx_hash: HashOf<InjectedTransaction>,
        receiver: oneshot::Receiver<SignedPromise>,
    ) -> Self {
        let timeout_duration = promise_waiting_timeout(db);
        let receiver = tokio::time::timeout(timeout_duration, receiver);
        Self { tx_hash, receiver }
    }

    pub fn into_parts(self) -> (HashOf<InjectedTransaction>, TimeoutReceiver) {
        (self.tx_hash, self.receiver)
    }
}

/// Tracks promise subscribers and dispatches incoming promises to them.
#[derive(Debug, Clone)]
pub struct PromiseSubscriptionManager {
    db: Database,
    subscribers: PromiseSubscribers,
}

impl PromiseSubscriptionManager {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            subscribers: PromiseSubscribers::default(),
        }
    }

    /// Register a new subscriber for `tx_hash`. Errors if one already
    /// exists for the same hash.
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

    /// Drop a subscriber registration for `tx_hash`, returning its
    /// sender if one existed (caller may use it to deliver a synthetic
    /// final value, or just let it drop).
    pub fn cancel_registration(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> Option<oneshot::Sender<SignedPromise>> {
        self.subscribers.remove(&tx_hash).map(|(_, v)| v)
    }

    /// Deliver `promise` to its registered subscriber, if any.
    pub fn dispatch_promise(&self, promise: SignedPromise) {
        let tx_hash = promise.data().tx_hash;
        let Some((_, sender)) = self.subscribers.remove(&tx_hash) else {
            warn!(?promise, "received promise with no registered subscriber");
            return;
        };
        match sender.send(promise.clone()) {
            Ok(()) => trace!(?promise, "sent promise to subscriber"),
            Err(promise) => trace!(?promise, "rpc promise receiver dropped"),
        }
    }

    #[cfg(test)]
    pub fn subscribers_count(&self) -> usize {
        self.subscribers.len()
    }
}

/// Maximum time a spawned [`PendingSubscriber`] will wait for a promise.
fn promise_waiting_timeout<DB: ConfigStorageRO>(db: &DB) -> std::time::Duration {
    let slot_duration_secs = db.config().timelines.slot.get();
    std::time::Duration::from_secs(slot_duration_secs * MAX_PROMISE_WAITING_SLOTS)
}
