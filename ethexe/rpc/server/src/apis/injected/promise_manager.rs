// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use anyhow::Result;
use ethexe_common::{
    Address, HashOf,
    db::{
        ConfigStorageRO, GlobalsStorageRO, InjectedStorageRO, InjectedStorageRW, OnChainStorageRO,
    },
    injected::{
        InjectedTransaction, Promise, ShieldedTransaction, SignedCompactTxReceipt, SignedTxReceipt,
        TransactionHash, TryFillPromiseResult, UnfilledPromiseReceipt, UpgradedReceipt,
    },
};
use ethexe_db::Database;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::oneshot;
use tracing::{trace, warn};

type PendingReceiptsCache = moka::sync::Cache<HashOf<InjectedTransaction>, UnfilledPromiseReceipt>;

#[derive(Debug)]
struct Subscriber {
    registration_hash: TransactionHash,
    sender: oneshot::Sender<SignedTxReceipt>,
}

/// Stores receipt subscribers under both their current routing hash and original registration hash.
///
/// A shielded transaction is initially routed by [`TransactionHash::Right`]. After unshielding,
/// its subscriber is moved to the corresponding [`TransactionHash::Left`] entry while retaining
/// the original shielded registration hash for cancellation.
#[derive(Debug, Default)]
struct PromiseSubscribers {
    /// Subscribers grouped by the hash under which an incoming receipt will currently arrive.
    ///
    /// Multiple subscribers can share an injected hash after a shielded subscriber is migrated
    /// to a hash that already has a directly registered injected subscriber.
    subscribers_by_receipt_hash: HashMap<TransactionHash, Vec<Subscriber>>,
    /// Maps each original registration hash to its current receipt hash in
    /// [`Self::subscribers_by_receipt_hash`].
    ///
    /// For an unmigrated subscriber both hashes are identical. For a migrated shielded
    /// subscriber this maps its shielded hash to the resulting injected hash.
    receipt_hash_by_registration_hash: HashMap<TransactionHash, TransactionHash>,
}

/// The manager for promise subscribers.
#[derive(Debug, Clone)]
pub struct PromiseSubscriptionManager {
    db: Database,
    /// Active subscribers for transaction receipts ([SignedTxReceipt]).
    subscribers: Arc<Mutex<PromiseSubscribers>>,
    /// Cached [UnfilledPromiseReceipt] waiting for local [Promise] computation.
    pending_receipts: PendingReceiptsCache,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum RegisterSubscriberError {
    #[error("Subscriber for this transaction already exists, tx_hash={0}")]
    AlreadyRegistered(TransactionHash),
}

type TimeoutReceiver = tokio::time::Timeout<oneshot::Receiver<SignedTxReceipt>>;

/// The pending [SignedTxReceipt] subscriber.
/// Subscriber will be spawned in separate tokio runtime task and will wait for promise.
///
/// Important: to avoid infinite waiting we wrap [oneshot::Receiver] into [tokio::time::timeout].
pub struct PendingSubscriber {
    /// Tx hash waiting promise for.
    tx_hash: TransactionHash,
    /// Wrapped tx receipt [oneshot::Receiver].
    receiver: TimeoutReceiver,
}

impl PendingSubscriber {
    pub fn new(
        db: &Database,
        tx_hash: TransactionHash,
        receiver: oneshot::Receiver<SignedTxReceipt>,
    ) -> Self {
        let timeout_duration = utils::receipt_waiting_timeout(db);
        let receiver = tokio::time::timeout(timeout_duration, receiver);
        Self { tx_hash, receiver }
    }

    pub fn into_parts(self) -> (TransactionHash, TimeoutReceiver) {
        (self.tx_hash, self.receiver)
    }
}

impl PromiseSubscriptionManager {
    pub fn new(db: Database) -> Self {
        Self {
            pending_receipts: utils::build_pending_receipts_cache(&db),
            db,
            subscribers: Arc::default(),
        }
    }

    // TODO: Issue #5402
    pub fn try_register_subscriber(
        &self,
        tx_hash: TransactionHash,
    ) -> Result<PendingSubscriber, RegisterSubscriberError> {
        let mut subscribers = self.subscribers.lock().expect("subscribers lock poisoned");
        if subscribers
            .receipt_hash_by_registration_hash
            .contains_key(&tx_hash)
        {
            return Err(RegisterSubscriberError::AlreadyRegistered(tx_hash));
        }

        let (sender, receiver) = oneshot::channel();
        subscribers
            .subscribers_by_receipt_hash
            .entry(tx_hash)
            .or_default()
            .push(Subscriber {
                registration_hash: tx_hash,
                sender,
            });
        subscribers
            .receipt_hash_by_registration_hash
            .insert(tx_hash, tx_hash);

        Ok(PendingSubscriber::new(&self.db, tx_hash, receiver))
    }

    pub fn cancel_registration(
        &self,
        tx_hash: TransactionHash,
    ) -> Option<oneshot::Sender<SignedTxReceipt>> {
        let mut subscribers = self.subscribers.lock().expect("subscribers lock poisoned");
        let receipt_hash = subscribers
            .receipt_hash_by_registration_hash
            .remove(&tx_hash)?;
        let receipt_subscribers = subscribers
            .subscribers_by_receipt_hash
            .get_mut(&receipt_hash)
            .expect("registered subscriber must exist");
        let position = receipt_subscribers
            .iter()
            .position(|subscriber| subscriber.registration_hash == tx_hash)
            .expect("registered subscriber must exist under its receipt hash");
        let subscriber = receipt_subscribers.swap_remove(position);
        if receipt_subscribers.is_empty() {
            subscribers
                .subscribers_by_receipt_hash
                .remove(&receipt_hash);
        }
        Some(subscriber.sender)
    }

    pub fn on_unshielded_transactions(
        &self,
        hash_mapping: Vec<(HashOf<ShieldedTransaction>, HashOf<InjectedTransaction>)>,
    ) {
        let moved_to = {
            let mut subscribers = self.subscribers.lock().expect("subscribers lock poisoned");
            let mut moved_to = Vec::new();

            for (shielded_hash, injected_hash) in hash_mapping {
                let registration_hash = TransactionHash::Right(shielded_hash);
                let receipt_hash = TransactionHash::Left(injected_hash);
                if subscribers
                    .receipt_hash_by_registration_hash
                    .get(&registration_hash)
                    != Some(&registration_hash)
                {
                    continue;
                }

                let moved = subscribers
                    .subscribers_by_receipt_hash
                    .remove(&registration_hash)
                    .expect("registered shielded subscriber must exist");
                subscribers
                    .subscribers_by_receipt_hash
                    .entry(receipt_hash)
                    .or_default()
                    .extend(moved);
                subscribers
                    .receipt_hash_by_registration_hash
                    .insert(registration_hash, receipt_hash);
                moved_to.push(injected_hash);
            }

            moved_to
        };

        for injected_hash in moved_to {
            if let Some(receipt) = self.db.receipt(injected_hash) {
                self.dispatch_receipt(receipt);
            }
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

    fn signer_is_known_validator(&self, address: Address, tx_hash: TransactionHash) -> bool {
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
        let senders = {
            let mut subscribers = self.subscribers.lock().expect("subscribers lock poisoned");
            subscribers
                .subscribers_by_receipt_hash
                .remove(&receipt.data().tx_hash())
                .unwrap_or_default()
                .into_iter()
                .map(|subscriber| {
                    subscribers
                        .receipt_hash_by_registration_hash
                        .remove(&subscriber.registration_hash);
                    subscriber.sender
                })
                .collect::<Vec<_>>()
        };

        for sender in senders {
            if let Err(unsent_receipt) = sender.send(receipt.clone()) {
                trace!("failed to send receipt to subscriber, receipt={unsent_receipt:?}");
            }
        }
    }

    fn store_and_dispatch_receipt(&self, receipt: SignedTxReceipt) {
        if matches!(receipt.data().tx_hash(), TransactionHash::Left(_)) {
            self.db.set_receipt(&receipt);
        }
        self.dispatch_receipt(receipt);
    }

    #[cfg(test)]
    pub fn subscribers_count(&self) -> usize {
        self.subscribers
            .lock()
            .expect("subscribers lock poisoned")
            .receipt_hash_by_registration_hash
            .len()
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
        injected::{InjectedTransaction, PurgedTransaction, Receipt, TransactionPurgedReason},
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
    ) -> std::pin::Pin<Box<oneshot::Receiver<SignedTxReceipt>>> {
        let pending = match manager.try_register_subscriber(TransactionHash::Left(tx_hash)) {
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

        let delivered = receiver.as_mut().await.unwrap();
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
        let delivered = receiver.as_mut().await.unwrap();
        let expected_receipt = Receipt::Promise(promise.clone());

        assert_eq!(delivered.data(), &Receipt::Promise(promise.clone()));
        assert_eq!(manager.subscribers_count(), 0);
        assert_eq!(db.promise(tx_hash), Some(promise));
        assert_eq!(
            db.receipt(tx_hash).unwrap().data().clone(),
            expected_receipt
        );
    }

    /// A duplicate registration for the same tx hash is rejected.
    #[tokio::test]
    async fn duplicate_subscriber_rejected() {
        let manager = PromiseSubscriptionManager::new(Database::memory());
        let (promise, _) = make_promise();
        let tx_hash = TransactionHash::Left(promise.tx_hash);
        let _first = manager.try_register_subscriber(tx_hash).ok();
        let err = manager
            .try_register_subscriber(tx_hash)
            .err()
            .expect("second registration must fail");
        assert!(matches!(err, RegisterSubscriberError::AlreadyRegistered(_)));
    }

    #[tokio::test]
    async fn shielded_subscriber_migrates_to_injected_hash() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let mut injected_receiver = register(&manager, promise.tx_hash);
        let shielded_hash = HashOf::<ShieldedTransaction>::random();
        let registration_hash = TransactionHash::Right(shielded_hash);
        let pending = manager
            .try_register_subscriber(registration_hash)
            .expect("first registration must succeed");
        let (_, receiver) = pending.into_parts();
        let mut receiver = Box::pin(receiver.into_inner());

        manager.on_unshielded_transactions(vec![(shielded_hash, promise.tx_hash)]);
        assert_eq!(manager.subscribers_count(), 2);

        manager.on_computed_promise(promise.clone());
        let receipt =
            SignedMessage::create(private_key, Receipt::Promise(promise.to_compact())).unwrap();
        set_current_validators(&db, vec![receipt.address()]);
        manager.on_tx_receipt(receipt.into());

        let expected = Receipt::Promise(promise);
        assert_eq!(receiver.as_mut().await.unwrap().data(), &expected);
        assert_eq!(injected_receiver.as_mut().await.unwrap().data(), &expected);
        assert_eq!(manager.subscribers_count(), 0);
    }

    #[tokio::test]
    async fn migration_dispatches_receipt_that_arrived_under_injected_hash_first() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let (promise, private_key) = make_promise();
        let shielded_hash = HashOf::<ShieldedTransaction>::random();
        let pending = manager
            .try_register_subscriber(TransactionHash::Right(shielded_hash))
            .expect("first registration must succeed");
        let (_, receiver) = pending.into_parts();
        let mut receiver = Box::pin(receiver.into_inner());

        manager.on_computed_promise(promise.clone());
        let receipt =
            SignedMessage::create(private_key, Receipt::Promise(promise.to_compact())).unwrap();
        set_current_validators(&db, vec![receipt.address()]);
        manager.on_tx_receipt(receipt.into());
        assert_eq!(manager.subscribers_count(), 1);
        assert!(db.receipt(promise.tx_hash).is_some());

        manager.on_unshielded_transactions(vec![(shielded_hash, promise.tx_hash)]);

        assert_eq!(
            receiver.as_mut().await.unwrap().data(),
            &Receipt::Promise(promise)
        );
        assert_eq!(manager.subscribers_count(), 0);
    }

    #[tokio::test]
    async fn shielded_purge_receipt_dispatches_without_database_storage() {
        let db = Database::memory();
        let manager = PromiseSubscriptionManager::new(db.clone());
        let shielded_hash = HashOf::<ShieldedTransaction>::random();
        let registration_hash = TransactionHash::Right(shielded_hash);
        let pending = manager
            .try_register_subscriber(registration_hash)
            .expect("first registration must succeed");
        let (_, receiver) = pending.into_parts();
        let mut receiver = Box::pin(receiver.into_inner());
        let purged = PurgedTransaction {
            tx_hash: registration_hash,
            reason: TransactionPurgedReason::DecryptionFailed,
        };
        let receipt = SignedMessage::create(
            PrivateKey::random(),
            Receipt::<ethexe_common::injected::CompactPromise>::Purged(purged.clone()),
        )
        .unwrap();
        set_current_validators(&db, vec![receipt.address()]);

        manager.on_tx_receipt(receipt.into());

        assert_eq!(
            receiver.as_mut().await.unwrap().data(),
            &Receipt::Purged(purged)
        );
        assert_eq!(manager.subscribers_count(), 0);
    }

    #[tokio::test]
    async fn migrated_shielded_subscriber_can_be_cancelled_by_original_hash() {
        let manager = PromiseSubscriptionManager::new(Database::memory());
        let shielded_hash = HashOf::<ShieldedTransaction>::random();
        let injected_hash = HashOf::<InjectedTransaction>::random();
        let registration_hash = TransactionHash::Right(shielded_hash);
        let _pending = manager
            .try_register_subscriber(registration_hash)
            .expect("first registration must succeed");

        manager.on_unshielded_transactions(vec![(shielded_hash, injected_hash)]);

        assert!(manager.cancel_registration(registration_hash).is_some());
        assert_eq!(manager.subscribers_count(), 0);
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

        let elapsed =
            tokio::time::timeout(std::time::Duration::from_millis(50), receiver.as_mut()).await;
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
}
