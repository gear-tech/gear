// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        ecdsa::PrivateKey,
        gear_core::{message::ReplyCode, rpc::ReplyInfo},
        injected::InjectedTransaction,
        mock::Mock,
    };

    fn make_promise() -> (Promise, PrivateKey) {
        let private_key = PrivateKey::random();
        let tx = InjectedTransaction::mock(());
        let tx_hash = tx.to_hash();
        let promise = Promise {
            tx_hash,
            reply: ReplyInfo {
                payload: vec![],
                value: 0,
                code: ReplyCode::Unsupported,
            },
        };
        (promise, private_key)
    }

    fn register(
        manager: &PromiseSubscriptionManager,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> std::pin::Pin<Box<oneshot::Receiver<SignedPromise>>> {
        let pending = match manager.try_register_subscriber(tx_hash) {
            Ok(pending) => pending,
            Err(err) => panic!("first registration must succeed: {err}"),
        };
        let (_, receiver) = pending.into_parts();
        // Inner oneshot::Receiver is Unpin; the outer Timeout is not,
        // hence we discard the timeout wrapper (tests drive their own
        // timing via tokio::time::timeout below).
        Box::pin(receiver.into_inner())
    }

    /// Producer signature lands after the local node has already
    /// computed the matching body — manager joins the two and delivers
    /// the full [`SignedPromise`] to the subscriber.
    #[tokio::test]
    async fn body_first_then_compact_dispatches() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let tx_hash = promise.tx_hash;

        let mut receiver = register(&manager, tx_hash);

        manager.on_computed_promise(promise.clone());
        assert_eq!(manager.subscribers_count(), 1);

        let compact = SignedCompactPromise::create_from_promise(private_key, &promise).unwrap();
        manager.on_compact_promise(compact.clone());
        let delivered = receiver.as_mut().await.unwrap();
        assert_eq!(delivered.data(), &promise);
        assert_eq!(manager.subscribers_count(), 0);
        assert_eq!(db.promise(tx_hash), Some(promise));
        assert_eq!(db.compact_promise(tx_hash), Some(compact));
    }

    /// Producer signature lands first via gossip; the manager parks it
    /// in `waiting_for_compute` and dispatches as soon as the local
    /// body lands.
    #[tokio::test]
    async fn compact_first_then_body_dispatches() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let tx_hash = promise.tx_hash;

        let mut receiver = register(&manager, tx_hash);

        let compact = SignedCompactPromise::create_from_promise(private_key, &promise).unwrap();
        manager.on_compact_promise(compact.clone());
        assert_eq!(manager.subscribers_count(), 1);

        manager.on_computed_promise(promise.clone());
        let delivered = receiver.as_mut().await.unwrap();
        assert_eq!(delivered.data(), &promise);
        assert_eq!(manager.subscribers_count(), 0);
        assert_eq!(db.promise(tx_hash), Some(promise));
        assert_eq!(db.compact_promise(tx_hash), Some(compact));
    }

    /// A duplicate registration for the same tx hash is rejected.
    #[tokio::test]
    async fn duplicate_subscriber_rejected() {
        let manager = PromiseSubscriptionManager::new(Database::memory());
        let (promise, _) = make_promise();
        let _first = manager.try_register_subscriber(promise.tx_hash).ok();
        let err = manager
            .try_register_subscriber(promise.tx_hash)
            .err()
            .expect("second registration must fail");
        assert!(matches!(err, RegisterSubscriberError::AlreadyRegistered(_)));
    }

    /// A compact promise whose signature does not match the body that
    /// arrives later is parked rather than delivering a malformed
    /// [`SignedPromise`].
    #[tokio::test]
    async fn compact_with_wrong_signature_is_parked() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let tx_hash = promise.tx_hash;

        let mut receiver = register(&manager, tx_hash);

        let other_promise = Promise {
            tx_hash,
            reply: ReplyInfo {
                payload: vec![1, 2, 3],
                value: 0,
                code: ReplyCode::Unsupported,
            },
        };
        let bad_compact =
            SignedCompactPromise::create_from_promise(private_key, &other_promise).unwrap();
        manager.on_compact_promise(bad_compact);
        manager.on_computed_promise(promise.clone());

        let elapsed =
            tokio::time::timeout(std::time::Duration::from_millis(50), receiver.as_mut()).await;
        assert!(elapsed.is_err(), "no signed promise should be delivered");
        assert_eq!(db.promise(tx_hash), Some(promise));
        assert_eq!(db.compact_promise(tx_hash), None);
    }
}
