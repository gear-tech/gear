// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use anyhow::Result;
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
    ) -> std::pin::Pin<Box<oneshot::Receiver<SignedTxReceipt>>> {
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
    /// in `waiting_for_compute` and dispatches as soon as the local
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
        let _first = manager.try_register_subscriber(promise.tx_hash).ok();
        let err = manager
            .try_register_subscriber(promise.tx_hash)
            .err()
            .expect("second registration must fail");
        assert!(matches!(err, RegisterSubscriberError::AlreadyRegistered(_)));
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
