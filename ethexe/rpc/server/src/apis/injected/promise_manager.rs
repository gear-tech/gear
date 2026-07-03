// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use dashmap::{DashMap, mapref::entry::Entry};
use ethexe_common::{
    Address, HashOf,
    db::{
        ConfigStorageRO, GlobalsStorageRO, InjectedStorageRO, InjectedStorageRW, OnChainStorageRO,
    },
    injected::{
        InjectedTransaction, Promise, SignedCompactTxReceipt, SignedTxReceipt,
        TryFillPromiseResult, UnfilledPromiseReceipt, UpgradedReceipt,
    },
};
use ethexe_db::Database;
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;
use tracing::{trace, warn};

/// Bounds how many concurrent watchers a single not-yet-resolved transaction can
/// accumulate, so one client cannot pin unbounded background tasks and map entries
/// on the server by opening many watches for the same transaction.
const MAX_SUBSCRIBERS_PER_TX: usize = 32;

/// `None` until `dispatch_receipt` publishes the receipt.
type ReceiptSlot = Option<Arc<SignedTxReceipt>>;
pub(crate) type ReceiptWatcher = watch::Receiver<ReceiptSlot>;
// One `watch` channel per transaction: subscribers are receiver clones, so a watcher
// cancels by dropping its receiver — no per-subscriber id bookkeeping needed.
type PromiseSubscribers = Arc<DashMap<HashOf<InjectedTransaction>, watch::Sender<ReceiptSlot>>>;
type PendingReceiptsCache = moka::sync::Cache<HashOf<InjectedTransaction>, UnfilledPromiseReceipt>;

/// The manager for promise subscribers.
#[derive(Debug, Clone)]
pub struct PromiseSubscriptionManager {
    db: Database,
    /// Active subscribers for injected transaction receipt ([SignedTxReceipt]).
    subscribers: PromiseSubscribers,
    /// Cached [UnfilledPromiseReceipt] waiting for local [Promise] computation.
    pending_receipts: PendingReceiptsCache,
}

pub enum RegisterSubscriberResult {
    Ready(SignedTxReceipt),
    Pending(PendingSubscriber),
    /// Too many concurrent watchers already registered for this transaction.
    TooManyWatchers,
}

/// The pending [SignedTxReceipt] subscriber.
/// Subscriber will be spawned in separate tokio runtime task and will wait for promise.
///
/// Important: to avoid infinite waiting the spawner bounds the wait by `timeout`.
pub struct PendingSubscriber {
    /// Tx hash waiting promise for.
    tx_hash: HashOf<InjectedTransaction>,
    receiver: ReceiptWatcher,
    /// Maximum time to wait for the receipt.
    timeout: Duration,
}

impl PendingSubscriber {
    pub fn new(
        db: &Database,
        tx_hash: HashOf<InjectedTransaction>,
        receiver: ReceiptWatcher,
    ) -> Self {
        Self {
            tx_hash,
            receiver,
            timeout: utils::receipt_waiting_timeout(db),
        }
    }

    pub fn into_parts(self) -> (HashOf<InjectedTransaction>, ReceiptWatcher, Duration) {
        (self.tx_hash, self.receiver, self.timeout)
    }
}

impl PromiseSubscriptionManager {
    pub fn new(db: Database) -> Self {
        Self {
            pending_receipts: utils::build_pending_receipts_cache(&db),
            db,
            subscribers: PromiseSubscribers::default(),
        }
    }

    pub fn try_register_subscriber(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> RegisterSubscriberResult {
        self.try_register_subscriber_inner(tx_hash, || {})
    }

    /// `after_insert` is a test-only seam: it runs between the subscriber-map insert
    /// and the race-closing recheck below, so a test can deterministically land a
    /// receipt in that exact window (see `race_window_receipt_is_still_served_ready`).
    fn try_register_subscriber_inner(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
        after_insert: impl FnOnce(),
    ) -> RegisterSubscriberResult {
        if let Some(receipt) = self.db.receipt(tx_hash) {
            return RegisterSubscriberResult::Ready(receipt);
        }

        let receiver = {
            let sender = self
                .subscribers
                .entry(tx_hash)
                .or_insert_with(|| watch::channel(None).0);
            if sender.receiver_count() >= MAX_SUBSCRIBERS_PER_TX {
                return RegisterSubscriberResult::TooManyWatchers;
            }
            sender.subscribe()
        };

        after_insert();

        // Recheck: `store_and_dispatch_receipt` persists before dispatching, so a receipt
        // landing mid-registration is visible here even if its dispatch ran before our insert.
        if let Some(receipt) = self.db.receipt(tx_hash) {
            drop(receiver);
            self.cleanup_tx_entry(tx_hash);
            return RegisterSubscriberResult::Ready(receipt);
        }

        RegisterSubscriberResult::Pending(PendingSubscriber::new(&self.db, tx_hash, receiver))
    }

    /// Releases a subscriber that never made it to a spawned watch task.
    pub fn release_subscriber(&self, subscriber: PendingSubscriber) {
        let tx_hash = subscriber.tx_hash;
        drop(subscriber);
        self.cleanup_tx_entry(tx_hash);
    }

    /// Drops the per-transaction entry once no live watchers remain.
    pub fn cleanup_tx_entry(&self, tx_hash: HashOf<InjectedTransaction>) {
        // `entry` holds the shard write lock, so the count check and removal are atomic w.r.t. registration.
        let Entry::Occupied(entry) = self.subscribers.entry(tx_hash) else {
            return;
        };
        if entry.get().receiver_count() == 0 {
            entry.remove();
        }
    }

    // TODO: Issue #5403
    pub fn on_tx_receipt(&self, receipt: SignedCompactTxReceipt) {
        trace!(?receipt, "received new compact receipt");

        if !self.compact_receipt_signed_by_known_validator(&receipt) {
            return;
        }

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
                    self.pending_receipts.insert(tx_hash, unfilled);
                }
            },
            None => {
                trace!("not found promise in database, waiting for computation...");
                self.pending_receipts.insert(tx_hash, unfilled_promise);
            }
        }
    }

    pub fn on_computed_promise(&self, promise: Promise) {
        trace!(?promise, "received new computed promise");
        self.db.set_promise(&promise);

        let Some(unfilled_promise) = self.pending_receipts.remove(&promise.tx_hash) else {
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
                self.pending_receipts.insert(unfilled.tx_hash, unfilled);
            }
        }
    }

    fn compact_receipt_signed_by_known_validator(&self, receipt: &SignedCompactTxReceipt) -> bool {
        self.signer_is_known_validator(receipt.address(), receipt.data().tx_hash())
    }

    fn signer_is_known_validator(
        &self,
        address: Address,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> bool {
        let timestamp = self.db.globals().latest_synced_eb.header.timestamp;
        let timelines = self.db.config().timelines;

        let Some(current_era) = timelines.era_from_ts(timestamp) else {
            warn!(
                %tx_hash,
                ?address,
                timestamp,
                "failed to calculate current era for tx receipt validator check"
            );
            return false;
        };

        let Some(current_validators) = self.db.validators(current_era) else {
            warn!(
                %tx_hash,
                ?address,
                current_era,
                "current validator set not found for tx receipt validator check"
            );
            return false;
        };

        let signer_is_known = current_validators.contains(&address)
            || current_era
                .checked_sub(1)
                .and_then(|previous_era| self.db.validators(previous_era))
                .is_some_and(|previous_validators| previous_validators.contains(&address));

        if !signer_is_known {
            trace!(
                %tx_hash,
                ?address,
                current_era,
                "tx receipt signer is not in the known validator set"
            );
        }

        signer_is_known
    }

    fn dispatch_receipt(&self, receipt: SignedTxReceipt) {
        let tx_hash = receipt.data().tx_hash();
        let Some((_tx_hash, sender)) = self.subscribers.remove(&tx_hash) else {
            return;
        };

        if sender.send(Some(Arc::new(receipt))).is_err() {
            trace!(%tx_hash, "no live subscribers left for the receipt");
        }
    }

    fn store_and_dispatch_receipt(&self, receipt: SignedTxReceipt) {
        self.db.set_receipt(&receipt);
        self.dispatch_receipt(receipt);
    }

    #[cfg(test)]
    pub fn subscribers_count(&self) -> usize {
        self.subscribers
            .iter()
            .map(|entry| entry.value().receiver_count())
            .sum()
    }
}

mod utils {
    use super::PendingReceiptsCache;
    use ethexe_common::{db::ConfigStorageRO, injected::VALIDITY_WINDOW};
    use std::time::Duration;

    /// The maximum number of slots RPC will wait for transaction promise.
    ///
    /// Reuse [VALIDITY_WINDOW] with a `2` slots reserve, because it defines
    /// the exact number of blocks within transaction is valid and promise can appear.
    const MAX_PROMISE_WAITING_SLOTS: u64 = VALIDITY_WINDOW as u64 + 2u64;
    /// The maximum number of pending receipts which are waiting for promise computation.
    const MAX_PENDING_RECEIPTS_CACHE_CAPACITY: u64 = 2_000;
    /// The default capacity of pending receipts cache.
    const DEFAULT_PENDING_RECEIPTS_CACHE_CAPACITY: usize = 100;

    /// Returns the maximum time that spawned [super::PendingSubscriber] will wait for promise.
    pub fn receipt_waiting_timeout<DB: ConfigStorageRO>(db: &DB) -> Duration {
        let slot_duration_secs = db.config().timelines.slot.get();
        Duration::from_secs(slot_duration_secs * MAX_PROMISE_WAITING_SLOTS)
    }

    /// Creates [`moka::sync::Cache`] instance for pending [UnfilledPromiseReceipt](super::UnfilledPromiseReceipt).
    pub fn build_pending_receipts_cache<DB: ConfigStorageRO>(db: &DB) -> PendingReceiptsCache {
        // Note: pending receipt will be removed from cache after `MAX_PROMISE_WAITING_SLOTS`,
        //       because after that time must be no active subscriber.
        let time_to_live = receipt_waiting_timeout(db);

        moka::sync::CacheBuilder::default()
            .initial_capacity(DEFAULT_PENDING_RECEIPTS_CACHE_CAPACITY)
            .max_capacity(MAX_PENDING_RECEIPTS_CACHE_CAPACITY)
            .time_to_live(time_to_live)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        Address, SignedMessage, ValidatorsVec,
        db::{GlobalsStorageRO, OnChainStorageRW, SetGlobals},
        ecdsa::PrivateKey,
        injected::{InjectedTransaction, Receipt},
        mock::Mock,
    };
    use gear_core::{message::ReplyCode, rpc::ReplyInfo};

    fn make_promise() -> (Promise, PrivateKey) {
        let private_key = PrivateKey::random();
        let tx = InjectedTransaction::mock(());
        let promise = Promise::mock(tx.to_hash());
        (promise, private_key)
    }

    fn set_current_validators(db: &Database, validators: Vec<Address>) {
        db.set_validators(
            0,
            ValidatorsVec::try_from(validators).expect("validators must be non-empty"),
        );
    }

    fn set_validators(db: &Database, era: u64, validators: Vec<Address>) {
        db.set_validators(
            era,
            ValidatorsVec::try_from(validators).expect("validators must be non-empty"),
        );
    }

    fn set_current_era(db: &Database, era: u64) {
        let mut globals = db.globals().clone();
        globals.latest_synced_eb.header.timestamp = era;
        db.set_globals(globals);
    }

    fn register(
        manager: &PromiseSubscriptionManager,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> ReceiptWatcher {
        let pending = match manager.try_register_subscriber(tx_hash) {
            RegisterSubscriberResult::Pending(pending) => pending,
            _ => panic!("empty database must produce a pending registration"),
        };
        let (_tx_hash, receiver, _timeout) = pending.into_parts();
        receiver
    }

    async fn next_receipt(receiver: &mut ReceiptWatcher) -> Arc<SignedTxReceipt> {
        receiver
            .wait_for(|receipt| receipt.is_some())
            .await
            .expect("receipt sender must be alive")
            .clone()
            .expect("`wait_for` guarantees the receipt is set")
    }

    /// Producer signature lands after the local node has already
    /// computed the matching body — manager joins the two and delivers
    /// the full [`SignedTxReceipt`] to the subscriber.
    #[tokio::test]
    async fn body_first_then_compact_dispatches() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let tx_hash = promise.tx_hash;

        let mut receiver = register(&manager, tx_hash);

        manager.on_computed_promise(promise.clone());
        assert_eq!(manager.subscribers_count(), 1);

        let receipt =
            SignedMessage::create(private_key, Receipt::Promise(promise.to_compact())).unwrap();
        set_current_validators(&db, vec![receipt.address()]);
        manager.on_tx_receipt(receipt.into());

        let delivered = next_receipt(&mut receiver).await;
        let expected_receipt = Receipt::Promise(promise.clone());
        assert_eq!(delivered.data().clone(), expected_receipt);
        assert_eq!(manager.subscribers_count(), 0);
        assert_eq!(db.promise(tx_hash), Some(promise));
        assert_eq!(
            db.receipt(tx_hash).unwrap().data().clone(),
            expected_receipt
        );
    }

    /// Producer signature lands first via gossip; the manager parks it
    /// in `pending_receipts` and dispatches as soon as the local
    /// body lands.
    #[tokio::test]
    async fn compact_first_then_body_dispatches() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let tx_hash = promise.tx_hash;

        let mut receiver = register(&manager, tx_hash);

        let receipt =
            SignedMessage::create(private_key, Receipt::Promise(promise.to_compact())).unwrap();
        set_current_validators(&db, vec![receipt.address()]);
        manager.on_tx_receipt(receipt.into());
        assert_eq!(manager.subscribers_count(), 1);

        manager.on_computed_promise(promise.clone());
        let delivered = next_receipt(&mut receiver).await;
        let expected_receipt = Receipt::Promise(promise.clone());

        assert_eq!(delivered.data(), &Receipt::Promise(promise.clone()));
        assert_eq!(manager.subscribers_count(), 0);
        assert_eq!(db.promise(tx_hash), Some(promise));
        assert_eq!(
            db.receipt(tx_hash).unwrap().data().clone(),
            expected_receipt
        );
    }

    /// A compact promise whose signature does not match the body that
    /// arrives later is parked rather than delivering a malformed
    /// [`SignedTxReceipt`].
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
        let bad_receipt =
            SignedMessage::create(private_key, Receipt::Promise(other_promise.to_compact()))
                .unwrap();
        set_current_validators(&db, vec![bad_receipt.address()]);
        manager.on_tx_receipt(bad_receipt.into());
        manager.on_computed_promise(promise.clone());

        let elapsed = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            receiver.wait_for(|receipt| receipt.is_some()),
        )
        .await;
        assert!(elapsed.is_err(), "no signed promise should be delivered");
        assert_eq!(db.promise(tx_hash), Some(promise));
        assert_eq!(db.receipt(tx_hash), None);
    }

    #[test]
    fn on_tx_receipt_ignores_non_validator_signature() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let tx_hash = promise.tx_hash;
        let receipt =
            SignedMessage::create(private_key, Receipt::Promise(promise.to_compact())).unwrap();

        set_current_validators(&db, vec![Address::from(1)]);
        db.set_promise(&promise);

        manager.on_tx_receipt(receipt.into());

        assert_eq!(db.receipt(tx_hash), None);
    }

    #[test]
    fn on_tx_receipt_accepts_validator_signature() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let tx_hash = promise.tx_hash;
        let receipt =
            SignedMessage::create(private_key, Receipt::Promise(promise.to_compact())).unwrap();

        set_current_validators(&db, vec![receipt.address()]);
        db.set_promise(&promise);

        manager.on_tx_receipt(receipt.into());

        assert_eq!(
            db.receipt(tx_hash).unwrap().data(),
            &Receipt::Promise(promise)
        );
    }

    #[test]
    fn on_tx_receipt_accepts_previous_era_validator_signature() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let tx_hash = promise.tx_hash;
        let receipt =
            SignedMessage::create(private_key, Receipt::Promise(promise.to_compact())).unwrap();

        set_current_era(&db, 1);
        set_validators(&db, 0, vec![receipt.address()]);
        set_validators(&db, 1, vec![Address::from(1)]);
        db.set_promise(&promise);

        manager.on_tx_receipt(receipt.into());

        assert_eq!(
            db.receipt(tx_hash).unwrap().data(),
            &Receipt::Promise(promise)
        );
    }

    #[test]
    fn on_tx_receipt_rejects_next_era_validator_signature() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let tx_hash = promise.tx_hash;
        let receipt =
            SignedMessage::create(private_key, Receipt::Promise(promise.to_compact())).unwrap();

        set_current_validators(&db, vec![Address::from(1)]);
        set_validators(&db, 1, vec![receipt.address()]);
        db.set_promise(&promise);

        manager.on_tx_receipt(receipt.into());

        assert_eq!(db.receipt(tx_hash), None);
    }

    #[tokio::test]
    async fn multiple_subscribers_receive_same_receipt() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let tx_hash = promise.tx_hash;

        let mut first = register(&manager, tx_hash);
        let mut second = register(&manager, tx_hash);

        manager.on_computed_promise(promise.clone());
        let receipt =
            SignedMessage::create(private_key, Receipt::Promise(promise.to_compact())).unwrap();
        set_current_validators(&db, vec![receipt.address()]);
        manager.on_tx_receipt(receipt.into());

        let first_receipt = next_receipt(&mut first).await;
        let second_receipt = next_receipt(&mut second).await;

        assert_eq!(first_receipt.data(), &Receipt::Promise(promise.clone()));
        assert_eq!(second_receipt.data(), &Receipt::Promise(promise));
        assert_eq!(manager.subscribers_count(), 0);
    }

    #[test]
    fn late_subscriber_gets_stored_receipt_without_registration() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let tx_hash = promise.tx_hash;
        let receipt: SignedTxReceipt =
            SignedMessage::create(private_key, Receipt::Promise(promise.clone()))
                .unwrap()
                .into();

        db.set_receipt(&receipt);

        match manager.try_register_subscriber(tx_hash) {
            RegisterSubscriberResult::Ready(ready) => assert_eq!(ready, receipt),
            _ => panic!("stored receipt must be returned immediately"),
        }

        assert_eq!(manager.subscribers_count(), 0);
    }

    #[tokio::test]
    async fn release_one_subscriber_keeps_other_subscriber() {
        let manager = PromiseSubscriptionManager::new(Database::memory());
        let (promise, _) = make_promise();
        let tx_hash = promise.tx_hash;

        let first = match manager.try_register_subscriber(tx_hash) {
            RegisterSubscriberResult::Pending(subscriber) => subscriber,
            _ => panic!("empty database must produce a pending registration"),
        };
        let second = match manager.try_register_subscriber(tx_hash) {
            RegisterSubscriberResult::Pending(subscriber) => subscriber,
            _ => panic!("empty database must produce a pending registration"),
        };

        assert_eq!(manager.subscribers_count(), 2);
        manager.release_subscriber(first);
        assert_eq!(manager.subscribers_count(), 1);
        manager.release_subscriber(second);
        assert_eq!(manager.subscribers_count(), 0);
    }

    /// Forces a receipt to land in the exact window the second `db.receipt` check in
    /// `try_register_subscriber` exists to close (between the subscriber-map insert and
    /// that recheck), via the `after_insert` test seam.
    #[test]
    fn race_window_receipt_is_still_served_ready() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let tx_hash = promise.tx_hash;
        let receipt: SignedTxReceipt =
            SignedMessage::create(private_key, Receipt::Promise(promise.clone()))
                .unwrap()
                .into();

        let result = manager.try_register_subscriber_inner(tx_hash, || {
            db.set_receipt(&receipt);
        });

        match result {
            RegisterSubscriberResult::Ready(ready) => assert_eq!(ready, receipt),
            _ => panic!("a receipt landing mid-registration must still be served as Ready"),
        }
        assert_eq!(
            manager.subscribers_count(),
            0,
            "the race-window registration must be rolled back"
        );
    }

    /// A single transaction cannot accumulate unbounded live watchers, and a released
    /// watcher frees its slot.
    #[tokio::test]
    async fn too_many_watchers_for_one_tx_hash_is_rejected() {
        let manager = PromiseSubscriptionManager::new(Database::memory());
        let (promise, _) = make_promise();
        let tx_hash = promise.tx_hash;

        let subscribers: Vec<_> = (0..MAX_SUBSCRIBERS_PER_TX)
            .map(|_| match manager.try_register_subscriber(tx_hash) {
                RegisterSubscriberResult::Pending(subscriber) => subscriber,
                _ => panic!("registrations under the cap must be pending"),
            })
            .collect();

        assert!(matches!(
            manager.try_register_subscriber(tx_hash),
            RegisterSubscriberResult::TooManyWatchers
        ));

        drop(subscribers);
        assert!(matches!(
            manager.try_register_subscriber(tx_hash),
            RegisterSubscriberResult::Pending(_)
        ));
    }
}
