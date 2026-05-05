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
    HashOf, SignedMessage, ToDigest,
    db::{InjectedStorageRO, InjectedStorageRW},
    injected::{
        InjectedTransaction, Promise, SignedCompactTxReceipt, SignedFullTxReceipt, TxReceipt,
    },
};
use ethexe_db::Database;
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{trace, warn};

// TODO: Issues #5384 and #5385.
type PromiseSubscribers =
    Arc<DashMap<HashOf<InjectedTransaction>, oneshot::Sender<SignedFullTxReceipt>>>;
type ReceiptsComputationWaiting = Arc<DashMap<HashOf<InjectedTransaction>, SignedCompactTxReceipt>>;

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

type TimeoutReceiver = tokio::time::Timeout<oneshot::Receiver<SignedFullTxReceipt>>;

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
        receiver: oneshot::Receiver<SignedFullTxReceipt>,
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
    ) -> Option<oneshot::Sender<SignedFullTxReceipt>> {
        self.subscribers.remove(&tx_hash).map(|(_, v)| v)
    }

    // TODO: Issue #5403
    pub fn on_tx_receipt(&self, receipt: SignedCompactTxReceipt) {
        trace!(?receipt, "received new compact promise");
        if receipt.data().is_error() {
            self.dispatch_receipt(receipt.as_full_receipt_error().expect("infallible"));
            return;
        }

        let tx_hash = receipt.data().tx_hash();
        match self.db.promise(tx_hash) {
            Some(promise) => match receipt.as_promise_with_signature() {
                Some((compact, signature, address)) => {
                    // self.db.set_compact_promise(&compact);
                    if compact.to_digest() == promise.to_digest() {
                        let message = unsafe {
                            SignedMessage::from_parts_unchecked(
                                TxReceipt::Promise(promise),
                                *signature,
                                address,
                            )
                        };
                        self.dispatch_receipt(message.into());
                    }
                }
                None => {
                    warn!(
                        ?receipt, %tx_hash, "failed to create signed receipt from parts, producer send invalid signature"
                    );
                    self.waiting_for_compute.insert(tx_hash, receipt);
                }
            },
            None => {
                trace!("not found promise in database, waiting for computation...");
                self.waiting_for_compute.insert(tx_hash, receipt);
            }
        }
    }

    pub fn on_computed_promise(&self, promise: Promise) {
        trace!(?promise, "received new computed promise");
        self.db.set_promise(&promise);

        if let Some((_, receipt)) = self.waiting_for_compute.remove(&promise.tx_hash) {
            match receipt.as_promise_with_signature() {
                Some((compact, signature, address)) => {
                    // self.db.set_compact_promise(&compact_promise)
                    if promise.to_digest() == compact.to_digest() {
                        let receipt = TxReceipt::Promise(promise);
                        let signed_receipt = unsafe {
                            SignedMessage::from_parts_unchecked(receipt, *signature, address)
                        };
                        self.dispatch_receipt(signed_receipt.into());
                    }
                }
                None => {
                    trace!(?receipt, tx_hash=?receipt.data().tx_hash(), "failed to create signed promise from parts");
                }
            }
        }
    }

    fn dispatch_receipt(&self, receipt: SignedFullTxReceipt) {
        if let Some((_, sender)) = self.subscribers.remove(&receipt.data().tx_hash())
            && let Err(unsent_receipt) = sender.send(receipt)
        {
            trace!("failed to send receipt to subscriber, receipt={unsent_receipt:?}");
        }
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
    pub fn promise_waiting_timeout<DB: ConfigStorageRO>(db: &DB) -> std::time::Duration {
        let slot_duration_secs = db.config().timelines.slot.get();
        std::time::Duration::from_secs(slot_duration_secs * MAX_PROMISE_WAITING_SLOTS)
    }
}
