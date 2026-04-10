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
    HashOf, SignedMessage,
    db::{InjectedStorageRO, InjectedStorageRW},
    injected::{CompactSignedPromise, InjectedTransaction, Promise, SignedPromise},
};
use ethexe_db::Database;
use std::{sync::Arc, time::Duration};
use tokio::sync::oneshot;
use tracing::trace;

const MAX_PROMISE_WAITING_SECS: u64 = alloy::eips::merge::SLOT_DURATION_SECS * 5;

// TODO idea: implement `PromisesHandle` that provides two methods: `on_computed_promise` and `on_compact_promise`.
// And provide this handle outside using `fn handle(&self) -> &PromiseHandle{}` to handle events in server.

type PromiseSubscribers = Arc<DashMap<HashOf<InjectedTransaction>, oneshot::Sender<SignedPromise>>>;
type PromisesComputationWaiting = Arc<DashMap<HashOf<InjectedTransaction>, CompactSignedPromise>>;

/// The manager for promise subscriptions.
#[derive(Debug, Clone)]
pub struct PromiseSubscriptionManager {
    db: Database,
    subscribers: PromiseSubscribers,

    waiting_for_compute: PromisesComputationWaiting,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RegisterWatcherError {
    #[error("Subscriber for this transaction already exists, tx_hash={0}")]
    AlreadyRegistered(HashOf<InjectedTransaction>),
}

type TimeoutReceiver = tokio::time::Timeout<oneshot::Receiver<SignedPromise>>;

pub struct PendingSubscription {
    tx_hash: HashOf<InjectedTransaction>,
    receiver: TimeoutReceiver,
}

impl PendingSubscription {
    pub fn new(tx_hash: HashOf<InjectedTransaction>, receiver: TimeoutReceiver) -> Self {
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

    pub fn watchers(&self) -> PromiseSubscribers {
        self.subscribers.clone()
    }

    pub fn try_register_watcher(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> Result<PendingSubscription, RegisterWatcherError> {
        match self.subscribers.entry(tx_hash) {
            Entry::Occupied(_) => Err(RegisterWatcherError::AlreadyRegistered(tx_hash)),
            Entry::Vacant(entry) => {
                let (sender, receiver) = oneshot::channel();
                let receiver =
                    tokio::time::timeout(Duration::from_secs(MAX_PROMISE_WAITING_SECS), receiver);

                entry.insert(sender);
                Ok(PendingSubscription::new(tx_hash, receiver))
            }
        }
    }

    pub fn cancel_registration(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> Option<oneshot::Sender<SignedPromise>> {
        self.subscribers.remove(&tx_hash).map(|(_, v)| v)
    }

    pub fn on_compact_promise(&self, compact: CompactSignedPromise) {
        let tx_hash = compact.data().tx_hash;
        match self.db.promise(tx_hash) {
            Some(promise) => match utils::try_build_signed_promise(promise, &compact) {
                Ok(signed_promise) => self.dispatch_promise(signed_promise),
                Err(_err) => todo!(),
            },
            None => {
                trace!("not found promise in database, waiting for computation...");
                self.waiting_for_compute.insert(tx_hash, compact);
            }
        }
    }

    pub fn on_computed_promise(&self, promise: Promise) {
        // Set computed promise to RPC database
        self.db.set_promise(&promise);

        if let Some((_, compact_promise)) = self.waiting_for_compute.remove(&promise.tx_hash) {
            match utils::try_build_signed_promise(promise, &compact_promise) {
                Ok(signed_promise) => self.dispatch_promise(signed_promise),
                Err(_err) => {} // handle error, maybe reinsert to map.
            }
        }
    }

    #[cfg(test)]
    pub fn subscribers_count(&self) -> usize {
        self.subscribers.len()
    }

    fn dispatch_promise(&self, promise: SignedPromise) {
        if let Some((_, sender)) = self.subscribers.remove(&promise.data().tx_hash)
            && let Err(unsent_promise) = sender.send(promise)
        {
            trace!("failed to send promise to subscriber, promise={unsent_promise:?}");
        }
    }
}

mod utils {
    use super::*;

    pub fn try_build_signed_promise(
        promise: Promise,
        compact: &CompactSignedPromise,
    ) -> Result<SignedPromise, &'static str> {
        let address = compact.address();
        let signature = *compact.signature();

        SignedMessage::try_from_parts(promise, signature, address)
    }
}
