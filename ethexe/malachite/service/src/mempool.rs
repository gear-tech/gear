// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Source of injected transactions for the Malachite producer.
//!
//! Two layers in this module:
//!
//! 1. The [`Mempool`] trait — abstract dependency consumed by
//!    [`crate::EthexeExternalities`] when [`ethexe_malachite_core::Externalities::build_block_above`]
//!    fires. Tests stub it with a crate-private `EmptyMempool`; production
//!    uses the [`InjectedTxMempool`] in this file.
//!
//! 2. [`InjectedTxMempool`] — the in-memory pool itself. Lifecycle
//!    rules (see also `ethexe-consensus/src/tx_validation.rs`):
//!
//!    - Every tx carries `reference_block: H256`. The tx is valid as
//!      long as `ref_block.height + VALIDITY_WINDOW > head.height`.
//!    - On insert we drop any tx whose `ref_block` is already outside
//!      the validity window relative to the latest observed head, or
//!      whose `ref_block` is not yet in the database.
//!    - On fetch we return only txs whose `ref_block` is a canonical
//!      ancestor of the given `head`. Non-ancestors are kept — a
//!      reorg can make them eligible again.
//!    - On forget (finalized MB) we remove the tx from the pool and
//!      remember its hash in a seen-hash table. Subsequent inserts
//!      of the same tx are rejected. Seen-hashes age out by the
//!      same `VALIDITY_WINDOW` rule as pool entries.
//!
//! The pool makes heavy use of `ethexe_db::Database::block_header` to
//! resolve `reference_block` into heights and to walk ancestor links;
//! all DB reads are synchronous and cheap (RocksDB point lookups).

use async_trait::async_trait;
use ethexe_common::{
    HashOf, SimpleBlockData,
    db::{GlobalsStorageRO, InjectedStorageRW, OnChainStorageRO},
    injected::{
        InjectedTransaction, PurgedTransaction, ShieldedTransaction, SignedInjectedTransaction,
        SignedShieldedTransaction, Transaction, TransactionAcceptance, TransactionHash,
        TransactionPurgedReason, TransactionRef, VALIDITY_WINDOW,
    },
};
use ethexe_db::Database;
use gprimitives::H256;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{Notify, RwLock};
use tracing::{info, trace};

/// Outcome of [`Mempool::insert`]: accept variants mean the tx is (now or
/// already) tracked by this validator; reject variants are terminal.
/// See [`Self::is_accepted`].
#[derive(Clone, Debug, PartialEq, Eq, derive_more::Display)]
pub enum TxInsertionStatus {
    // ---- Accept ----
    /// Fresh insert — the tx just entered the pool.
    #[display("inserted")]
    Inserted,
    /// Same tx hash already lives in the pool — idempotent no-op.
    #[display("already in pool")]
    AlreadyInPool,
    /// Same tx hash was already committed within the validity window — idempotent no-op.
    #[display("already included within validity window")]
    AlreadyIncluded,
    // ---- Reject ----
    /// `reference_block` is past the validity window relative to the latest head.
    #[display("reference_block past validity window")]
    ExpiredRefBlock,
    /// Pool is at capacity.
    #[display("mempool at capacity")]
    PoolFull,
    /// Non-zero-value injected transactions are not yet supported (#5083).
    #[display("non-zero value injected txs are not yet supported (#5083)")]
    NonZeroValue,
}

impl TxInsertionStatus {
    /// True for variants where the tx is (or was already) tracked by
    /// this validator — callers' promise subscriptions stay valid.
    pub fn is_accepted(&self) -> bool {
        matches!(
            self,
            Self::Inserted | Self::AlreadyInPool | Self::AlreadyIncluded,
        )
    }
}

impl From<TxInsertionStatus> for TransactionAcceptance {
    fn from(status: TxInsertionStatus) -> Self {
        if status.is_accepted() {
            Self::Accept
        } else {
            Self::Reject {
                reason: status.to_string(),
            }
        }
    }
}

/// Producer-side source of injected transactions. Fetch is non-destructive;
/// `forget` runs after MB finalization and dedups within `VALIDITY_WINDOW`.
#[async_trait]
pub trait Mempool: Send + Sync + 'static {
    /// Attempt to insert a new transaction into the pool.
    async fn insert(&self, tx: Transaction) -> TxInsertionStatus;

    /// Notify the pool of a new chain head; evicts expired entries
    /// and returns the purged transactions.
    async fn set_chain_head(&self, head: SimpleBlockData) -> Vec<PurgedTransaction>;

    /// Txs whose `reference_block` is an ancestor of `head`.
    async fn fetch(&self, head: SimpleBlockData) -> Vec<Transaction>;

    /// Drop committed txs and remember their hashes for dedup.
    async fn forget(&self, committed: &[TransactionRef<'_>]);

    /// Best-effort wake-up on new tx; spurious wake-ups allowed.
    async fn wait_for_new_tx(&self);
}

/// Always-empty mempool — used by in-crate unit tests to drive
/// externalities without spinning up the real pool. Kept out of the
/// public API so consumers can't reach for a no-op pool in production.
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct EmptyMempool;

#[cfg(test)]
#[async_trait]
impl Mempool for EmptyMempool {
    async fn insert(&self, _tx: Transaction) -> TxInsertionStatus {
        TxInsertionStatus::Inserted
    }

    async fn set_chain_head(&self, _head: SimpleBlockData) -> Vec<PurgedTransaction> {
        Vec::new()
    }

    async fn fetch(&self, _head: SimpleBlockData) -> Vec<Transaction> {
        Vec::new()
    }

    async fn forget(&self, _committed: &[TransactionRef<'_>]) {}

    async fn wait_for_new_tx(&self) {
        std::future::pending().await
    }
}

/// Default cap on the number of pending TXs the in-memory pool holds.
pub const DEFAULT_POOL_CAPACITY: usize = 10_000;

// TODO: #5474 a single signer can fill `DEFAULT_POOL_CAPACITY` with
// distinct-salt valid txs and starve out the rest of the network until
// `VALIDITY_WINDOW` ages them out. Needs a per-sender quota keyed on the
// recovered ECDSA address.

/// Pool state behind a single lock — operations are short, contention low.
#[derive(Debug, Default)]
struct Inner {
    /// Injected transactions waiting for its inclusion in chain.
    injected_pool: HashMap<HashOf<InjectedTransaction>, SignedInjectedTransaction>,
    /// Recently committed injected txs (tx_hash → ref_block) for dedup. Aged out with the validity window.
    injected_seen: HashMap<HashOf<InjectedTransaction>, H256>,
    /// Shielded transactions waiting for its inclusion in chain.
    shielded_pool: HashMap<HashOf<ShieldedTransaction>, SignedShieldedTransaction>,
    /// Recently committed shielded txs (tx_hash → ref_block) for dedup. Aged out with the validity window.
    shielded_seen: HashMap<HashOf<ShieldedTransaction>, H256>,
    /// Latest chain head height — drives age-out of pool/seen entries.
    latest_head_height: Option<u32>,
}

impl Inner {
    /// Returns number of transactions in `injected_pool` + `shielded_pool`.
    pub fn len(&self) -> usize {
        self.injected_pool.len() + self.shielded_pool.len()
    }
}

/// In-memory injected-tx pool backed by the node DB for ref-block resolution.
#[derive(Debug)]
pub struct InjectedTxMempool {
    /// Mutable pool state.
    inner: RwLock<Inner>,
    /// DB for resolving `reference_block` heights and ancestor walks.
    db: Database,
    /// Max number of pending transactions.
    capacity: usize,
    /// Notification about new transactions in `wait_for_new_tx`.
    new_tx_notify: Arc<Notify>,
}

impl InjectedTxMempool {
    pub fn new(db: Database) -> Self {
        Self::with_capacity(db, DEFAULT_POOL_CAPACITY)
    }

    pub fn with_capacity(db: Database, capacity: usize) -> Self {
        Self {
            inner: RwLock::new(Inner::default()),
            db,
            capacity,
            new_tx_notify: Arc::new(Notify::new()),
        }
    }

    /// Delegates call to `Inner::len`.
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Resolve `reference_block` to its canonical height via the DB.
    /// Returns `None` if the block isn't in the DB yet.
    fn ref_block_height(&self, reference_block: H256) -> Option<u32> {
        self.db.block_header(reference_block).map(|h| h.height)
    }

    /// True when `ref_block` is past the validity window for `head_height`.
    fn is_expired(head_height: u32, ref_block_height: u32) -> bool {
        ref_block_height.saturating_add(VALIDITY_WINDOW as u32) <= head_height
    }

    /// Oldest block the local DB has a header for; walks stop here.
    fn start_block_hash(&self) -> H256 {
        self.db.globals().start_block_hash
    }

    /// Set of ancestors of `head` within `VALIDITY_WINDOW` steps.
    fn recent_ancestors(&self, head_eb: &SimpleBlockData) -> HashSet<H256> {
        let start_fence = self.start_block_hash();

        let mut ancestors = HashSet::with_capacity(VALIDITY_WINDOW as usize + 1);
        ancestors.insert(head_eb.hash);

        let mut current = head_eb.hash;
        let mut parent = head_eb.header.parent_hash;
        for _ in 0..VALIDITY_WINDOW {
            if current == start_fence || parent == H256::zero() {
                break;
            }
            if !ancestors.insert(parent) {
                // Parent already visited — defensive cycle guard.
                break;
            }
            let Some(header) = self.db.block_header(parent) else {
                break;
            };
            current = parent;
            parent = header.parent_hash;
        }
        ancestors
    }

    /// Evict pool entries and seen-hashes whose `reference_block` has
    /// aged out relative to `head_height`.
    fn purge_expired(inner: &mut Inner, head_height: u32, db: &Database) -> Vec<PurgedTransaction> {
        let keep_seen = |tx_type: &'static str, tx_hash: H256, reference_block: &H256| match db
            .block_header(*reference_block)
            .map(|header| header.height)
        {
            Some(height) if !Self::is_expired(head_height, height) => true,
            _ => {
                trace!(%tx_type, %tx_hash, %reference_block, "dropping expired seen-hash");
                false
            }
        };
        inner
            .injected_seen
            .retain(|tx_hash, ref_block| keep_seen("injected", tx_hash.inner(), ref_block));
        inner
            .shielded_seen
            .retain(|tx_hash, ref_block| keep_seen("shielded", tx_hash.inner(), ref_block));

        let mut purged_txs = Vec::new();
        let mut purge_fn = |tx_hash: TransactionHash, ref_block: H256| match db
            .block_header(ref_block)
            .map(|h| h.height)
        {
            Some(h) if !Self::is_expired(head_height, h) => true,
            Some(h) => {
                trace!(
                    %tx_hash, %ref_block, ref_height = h, head_height,
                    "dropping expired tx from pool",
                );
                purged_txs.push(PurgedTransaction {
                    tx_hash,
                    reason: TransactionPurgedReason::Outdated,
                });
                false
            }
            None => {
                trace!(
                    %tx_hash, %ref_block,
                    "dropping tx with unknown ref_block from pool",
                );
                purged_txs.push(PurgedTransaction {
                    tx_hash,
                    reason: TransactionPurgedReason::UnknownReferenceBlock,
                });
                false
            }
        };

        inner.injected_pool.retain(|tx_hash, tx| {
            purge_fn(TransactionHash::Left(*tx_hash), tx.data().reference_block)
        });
        inner.shielded_pool.retain(|tx_hash, tx| {
            purge_fn(TransactionHash::Right(*tx_hash), tx.data().reference_block)
        });

        purged_txs
    }

    async fn insert_injected(&self, tx: SignedInjectedTransaction) -> TxInsertionStatus {
        let tx_hash = tx.data().to_hash();
        let ref_block = tx.data().reference_block;

        // Reject non-zero-value txs unconditionally (#5083 — value-bearing
        // injected txs are not supported yet). Done first so a malicious
        // sender can't burn pool capacity with txs that will never be
        // selectable.
        if tx.data().value != 0 {
            info!(
                %tx_hash,
                value = tx.data().value,
                "mempool: rejecting tx — non-zero value (#5083 not supported)",
            );
            return TxInsertionStatus::NonZeroValue;
        }

        let inner = self.inner.read().await;

        if inner.injected_seen.contains_key(&tx_hash) {
            info!(%tx_hash, "mempool: idempotent no-op — hash already committed within validity window");
            return TxInsertionStatus::AlreadyIncluded;
        }

        if inner.injected_pool.contains_key(&tx_hash) {
            info!(%tx_hash, pool_len = inner.len(), "mempool: idempotent no-op — duplicate insert");
            return TxInsertionStatus::AlreadyInPool;
        }

        // Unknown ref_block is accepted (filtered at fetch time);
        // reject only when it is known AND already past the validity window.
        let ref_height_opt = self.ref_block_height(ref_block);
        if let Some(ref_height) = ref_height_opt
            && let Some(head_height) = inner.latest_head_height
            && Self::is_expired(head_height, ref_height)
        {
            info!(
                %tx_hash, %ref_block, ref_height, head_height,
                "mempool: rejecting tx — reference_block past VALIDITY_WINDOW"
            );
            return TxInsertionStatus::ExpiredRefBlock;
        }

        if inner.len() >= self.capacity {
            info!(%tx_hash, capacity = self.capacity, "mempool: rejecting tx — pool at capacity");
            return TxInsertionStatus::PoolFull;
        }

        // Drop the lock around the DB write so concurrent operations don't
        // serialise behind disk I/O; the gates are re-checked after re-acquire.
        drop(inner);

        // TODO: #5489 remove, set in db only after mb finalization
        // Persist the tx so the local RPC can serve it by hash later.
        self.db.set_injected_transaction(tx.clone());

        let mut inner = self.inner.write().await;

        // Recheck dedup / capacity after the lock-free window.
        if inner.injected_seen.contains_key(&tx_hash) {
            return TxInsertionStatus::AlreadyIncluded;
        }
        if inner.injected_pool.contains_key(&tx_hash) {
            return TxInsertionStatus::AlreadyInPool;
        }
        if inner.len() >= self.capacity {
            return TxInsertionStatus::PoolFull;
        }

        let pool_len_after = inner.len() + 1;
        inner.injected_pool.insert(tx_hash, tx);
        info!(
            %tx_hash,
            %ref_block,
            ref_height = ?ref_height_opt,
            pool_len = pool_len_after,
            "mempool: insert accepted",
        );

        // Drop the lock before signaling so a resumed waiter doesn't bounce on it.
        drop(inner);
        self.new_tx_notify.notify_one();
        TxInsertionStatus::Inserted
    }

    async fn insert_shielded(&self, tx: SignedShieldedTransaction) -> TxInsertionStatus {
        let tx_hash = tx.data().to_hash();
        let ref_block = tx.data().reference_block;
        let inner = self.inner.read().await;

        if inner.shielded_seen.contains_key(&tx_hash) {
            info!(tx_hash = %tx_hash.inner(), "mempool: idempotent no-op — shielded hash already committed within validity window");
            return TxInsertionStatus::AlreadyIncluded;
        }

        if inner.shielded_pool.contains_key(&tx_hash) {
            info!(tx_hash = %tx_hash.inner(), pool_len = inner.len(), "mempool: idempotent no-op — duplicate shielded insert");
            return TxInsertionStatus::AlreadyInPool;
        }

        let ref_height_opt = self.ref_block_height(ref_block);
        if let Some(ref_height) = ref_height_opt
            && let Some(head_height) = inner.latest_head_height
            && Self::is_expired(head_height, ref_height)
        {
            info!(
                tx_hash = %tx_hash.inner(), %ref_block, ref_height, head_height,
                "mempool: rejecting shielded tx — reference_block past VALIDITY_WINDOW"
            );
            return TxInsertionStatus::ExpiredRefBlock;
        }

        if inner.len() >= self.capacity {
            info!(tx_hash = %tx_hash.inner(), capacity = self.capacity, "mempool: rejecting shielded tx — pool at capacity");
            return TxInsertionStatus::PoolFull;
        }
        drop(inner);

        let mut inner = self.inner.write().await;
        if inner.shielded_seen.contains_key(&tx_hash) {
            return TxInsertionStatus::AlreadyIncluded;
        }
        if inner.shielded_pool.contains_key(&tx_hash) {
            return TxInsertionStatus::AlreadyInPool;
        }
        if inner.len() >= self.capacity {
            return TxInsertionStatus::PoolFull;
        }

        let pool_len_after = inner.len() + 1;
        inner.shielded_pool.insert(tx_hash, tx);
        info!(
            tx_hash = %tx_hash.inner(),
            %ref_block,
            ref_height = ?ref_height_opt,
            pool_len = pool_len_after,
            "mempool: shielded insert accepted",
        );

        drop(inner);
        self.new_tx_notify.notify_one();
        TxInsertionStatus::Inserted
    }
}

#[async_trait]
impl Mempool for InjectedTxMempool {
    async fn insert(&self, tx: Transaction) -> TxInsertionStatus {
        match tx {
            Transaction::Injected(tx) => self.insert_injected(tx).await,
            Transaction::Shielded(tx) => self.insert_shielded(tx).await,
        }
    }

    async fn set_chain_head(&self, head: SimpleBlockData) -> Vec<PurgedTransaction> {
        let mut inner = self.inner.write().await;
        let h = head.header.height;
        if inner.latest_head_height == Some(h) {
            // Same height re-sent — nothing to GC beyond what we
            // already did on the previous call.
            return Default::default();
        }
        inner.latest_head_height = Some(h);
        Self::purge_expired(&mut inner, h, &self.db)
    }

    async fn fetch(&self, head: SimpleBlockData) -> Vec<Transaction> {
        let ancestors = self.recent_ancestors(&head);

        let inner = self.inner.read().await;
        let pool_len = inner.len();

        let mut transactions = Vec::new();
        inner
            .injected_pool
            .values()
            .filter(|tx| ancestors.contains(&tx.data().reference_block))
            .for_each(|tx| transactions.push(Transaction::Injected(tx.clone())));

        inner
            .shielded_pool
            .values()
            .filter(|tx| ancestors.contains(&tx.data().reference_block))
            .for_each(|tx| transactions.push(Transaction::Shielded(tx.clone())));

        info!(
            head_hash = %head.hash,
            head_height = head.header.height,
            ancestors = ancestors.len(),
            pool_len,
            returned = transactions.len(),
            "mempool: fetch",
        );
        transactions
    }

    async fn forget(&self, committed: &[TransactionRef<'_>]) {
        let mut inner = self.inner.write().await;
        committed.iter().for_each(|tx_ref| match tx_ref {
            TransactionRef::Injected(tx) => {
                let tx_hash = tx.data().to_hash();
                inner.injected_pool.remove(&tx_hash);
                inner
                    .injected_seen
                    .insert(tx_hash, tx.data().reference_block);
            }
            TransactionRef::Shielded(tx) => {
                let tx_hash = tx.data().to_hash();
                inner.shielded_pool.remove(&tx_hash);
                inner
                    .shielded_seen
                    .insert(tx_hash, tx.data().reference_block);
            }
        });
    }

    async fn wait_for_new_tx(&self) {
        // `notify_one` preserves a permit when no waiter is parked, so an
        // insert racing this call still wakes it. Spurious wake-ups allowed.
        self.new_tx_notify.notified().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        BlockHeader, PrivateKey, SignedMessage, SimpleBlockData,
        db::{BlockMetaStorageRW, GlobalsStorageRW, OnChainStorageRW},
        injected::{
            InjectedTransaction, SignedInjectedTransaction, SignedShieldedTransaction,
            TransactionAcceptance,
        },
    };
    use gprimitives::ActorId;
    use std::time::Duration;

    /// Pins the `TxInsertionStatus -> TransactionAcceptance` split.
    /// Adding a variant without updating [`TxInsertionStatus::is_accepted`]
    /// will be caught here.
    #[test]
    fn status_to_acceptance_mapping() {
        for status in [
            TxInsertionStatus::Inserted,
            TxInsertionStatus::AlreadyInPool,
            TxInsertionStatus::AlreadyIncluded,
        ] {
            assert!(status.is_accepted(), "{status:?} must classify as accepted");
            assert_eq!(
                TransactionAcceptance::from(status),
                TransactionAcceptance::Accept,
            );
        }
        for status in [
            TxInsertionStatus::NonZeroValue,
            TxInsertionStatus::PoolFull,
            TxInsertionStatus::ExpiredRefBlock,
        ] {
            assert!(
                !status.is_accepted(),
                "{status:?} must classify as rejected",
            );
            let reason = status.to_string();
            assert_eq!(
                TransactionAcceptance::from(status),
                TransactionAcceptance::Reject { reason },
            );
        }
    }

    #[tokio::test]
    async fn insert_rejects_non_zero_value_before_pool_state_checks() {
        // Verifies NonZeroValue fires *before* the pool is consulted —
        // a tx with value != 0 must never reach the seen / duplicate /
        // capacity gates. We seed the pool to capacity first to make
        // sure those gates would fire if reached.
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::with_capacity(db, 1);
        let pk = PrivateKey::random();

        // Fill to capacity with a valid tx so PoolFull would normally fire.
        pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 0).into())
            .await;

        let value_tx = SignedMessage::create(
            pk.clone(),
            InjectedTransaction {
                destination: ActorId::zero(),
                payload: vec![1, 2, 3].try_into().unwrap(),
                value: 42,
                reference_block: chain[1].hash,
                salt: vec![9; 32].try_into().unwrap(),
            },
        )
        .unwrap();

        assert_eq!(
            pool.insert(value_tx.into()).await,
            TxInsertionStatus::NonZeroValue
        );
        assert_eq!(
            pool.len().await,
            1,
            "non-zero-value tx must not enter the pool"
        );
    }

    /// Fresh insert that passes every gate must return `Inserted`.
    #[tokio::test]
    async fn insert_returns_inserted_for_fresh_tx() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();
        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 0);

        assert_eq!(pool.insert(tx.into()).await, TxInsertionStatus::Inserted);
        assert_eq!(pool.len().await, 1);
    }

    /// Same tx inserted twice — second insert hits the pool table and
    /// returns `AlreadyInPool` without bumping the size.
    #[tokio::test]
    async fn insert_returns_already_in_pool_for_duplicate() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();
        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 5);

        assert_eq!(
            pool.insert(tx.clone().into()).await,
            TxInsertionStatus::Inserted
        );
        assert_eq!(
            pool.insert(tx.into()).await,
            TxInsertionStatus::AlreadyInPool
        );
        assert_eq!(pool.len().await, 1);
    }

    /// After `forget`, re-inserting the same tx hits the seen-hash table
    /// and returns `AlreadyIncluded`.
    #[tokio::test]
    async fn insert_returns_already_included_for_committed_tx() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();
        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 11);

        pool.insert(tx.clone().into()).await;
        pool.forget(std::slice::from_ref(&TransactionRef::Injected(&tx)))
            .await;
        assert_eq!(pool.len().await, 0);

        assert_eq!(
            pool.insert(tx.into()).await,
            TxInsertionStatus::AlreadyIncluded
        );
        assert_eq!(pool.len().await, 0);
    }

    /// `ExpiredRefBlock` fires once `set_chain_head` has advanced past
    /// `ref_block_height + VALIDITY_WINDOW` and the tx is brand new.
    #[tokio::test]
    async fn insert_returns_expired_ref_block() {
        let db = Database::memory();
        let chain = linear_chain(&db, (VALIDITY_WINDOW as usize) + 5);
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();

        // Advance head so block 1 is past the validity window.
        let head_idx = (VALIDITY_WINDOW as usize) + 1;
        let _ = pool.set_chain_head(chain[head_idx]).await;

        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 0);
        assert_eq!(
            pool.insert(tx.into()).await,
            TxInsertionStatus::ExpiredRefBlock
        );
        assert_eq!(pool.len().await, 0);
    }

    /// Persist a synthetic linear chain of length `len` into the DB.
    /// Returns blocks oldest-first; first block has parent_hash = 0
    /// (genesis-like), later ones link to the previous hash.
    fn linear_chain(db: &Database, len: usize) -> Vec<SimpleBlockData> {
        let mut chain = Vec::with_capacity(len);
        let mut parent = H256::zero();
        for i in 0..len {
            let mut hb = [0u8; 32];
            hb[0] = 0x10 + (i as u8 % 0xF0);
            hb[1] = (i >> 8) as u8;
            hb[2] = i as u8;
            let hash = H256::from(hb);
            let header = BlockHeader {
                height: i as u32,
                timestamp: i as u64,
                parent_hash: parent,
            };
            db.set_block_header(hash, header);
            db.mutate_block_meta(hash, |_| {});
            chain.push(SimpleBlockData { hash, header });
            parent = hash;
        }
        chain
    }

    fn signed_tx(
        pk: &PrivateKey,
        destination: ActorId,
        ref_block: H256,
        salt: u8,
    ) -> SignedInjectedTransaction {
        SignedMessage::create(
            pk.clone(),
            InjectedTransaction {
                destination,
                payload: vec![1, 2, 3].try_into().unwrap(),
                value: 0,
                reference_block: ref_block,
                salt: vec![salt; 32].try_into().unwrap(),
            },
        )
        .unwrap()
    }

    fn signed_shielded_tx(
        pk: &PrivateKey,
        destination: ActorId,
        ref_block: H256,
        salt: u8,
    ) -> SignedShieldedTransaction {
        let injected_tx = InjectedTransaction {
            destination,
            payload: vec![1, 2, 3].try_into().unwrap(),
            value: 0,
            reference_block: ref_block,
            salt: vec![salt; 32].try_into().unwrap(),
        };
        let mut rng = gear_tdec::rand_utils::test_rng();
        let dealer_out = gear_tdec::deal::<gear_tdec::bls12_381::E>(3, 2, &mut rng);
        let shielded_tx = injected_tx
            .shield(&dealer_out.public_key, &mut rng)
            .unwrap();

        SignedMessage::create(pk.clone(), shielded_tx).unwrap()
    }

    #[tokio::test]
    async fn insert_unknown_ref_block_is_accepted() {
        let db = Database::memory();
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();
        let tx = signed_tx(&pk, ActorId::zero(), H256::random(), 1);
        pool.insert(tx.into()).await;
        assert_eq!(pool.len().await, 1);
    }

    #[tokio::test]
    async fn insert_then_fetch_round_trip() {
        let db = Database::memory();
        let chain = linear_chain(&db, 3);
        let pool = InjectedTxMempool::new(db);

        let pk = PrivateKey::random();
        let tx: Transaction = signed_tx(&pk, ActorId::zero(), chain[2].hash, 1).into();
        let tx_hash = tx.as_ref().hash();

        pool.insert(tx.clone()).await;
        assert_eq!(pool.len().await, 1);

        // The pool fetches when ref_block is on the canonical chain
        // of the head we hand it.
        let head = chain[2];
        let fetched = pool.fetch(head).await;
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].as_ref().hash(), tx_hash);
    }

    #[tokio::test]
    async fn capacity_limit_blocks_further_inserts() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::with_capacity(db, 2);

        let pk = PrivateKey::random();
        pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 0).into())
            .await;
        pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 1).into())
            .await;
        assert_eq!(
            pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 2).into())
                .await,
            TxInsertionStatus::PoolFull,
        );
        assert_eq!(
            pool.len().await,
            2,
            "third insert must hit the capacity cap"
        );
    }

    #[tokio::test]
    async fn capacity_is_shared_between_injected_and_shielded_pools() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::with_capacity(db, 1);
        let pk = PrivateKey::random();

        pool.insert(signed_shielded_tx(&pk, ActorId::zero(), chain[1].hash, 0).into())
            .await;

        assert_eq!(
            pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 1).into())
                .await,
            TxInsertionStatus::PoolFull,
        );
        assert_eq!(pool.len().await, 1);
    }

    #[tokio::test]
    async fn shielded_insert_fetch_and_forget_round_trip() {
        let db = Database::memory();
        let chain = linear_chain(&db, 3);
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();
        let tx: Transaction = signed_shielded_tx(&pk, ActorId::zero(), chain[2].hash, 1).into();
        let Transaction::Shielded(signed) = &tx else {
            unreachable!("helper creates shielded transaction");
        };
        let tx_hash = signed.data().to_hash();

        assert_eq!(pool.insert(tx.clone()).await, TxInsertionStatus::Inserted);
        assert_eq!(pool.len().await, 1);

        let fetched = pool.fetch(chain[2]).await;
        assert_eq!(fetched.len(), 1);
        let Transaction::Shielded(fetched) = &fetched[0] else {
            panic!("expected shielded transaction");
        };
        assert_eq!(fetched.data().to_hash(), tx_hash);

        pool.forget(std::slice::from_ref(&tx.as_ref())).await;
        assert_eq!(pool.len().await, 0);
        assert_eq!(pool.insert(tx).await, TxInsertionStatus::AlreadyIncluded);
        assert_eq!(pool.len().await, 0);
    }

    #[tokio::test]
    async fn set_chain_head_purges_expired_shielded() {
        let db = Database::memory();
        let chain = linear_chain(&db, (VALIDITY_WINDOW as usize) + 5);
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();
        let tx: Transaction = signed_shielded_tx(&pk, ActorId::zero(), chain[1].hash, 0).into();
        pool.insert(tx).await;
        assert_eq!(pool.len().await, 1);

        let head_idx = (VALIDITY_WINDOW as usize) + 1;
        let _ = pool.set_chain_head(chain[head_idx]).await;
        assert_eq!(pool.len().await, 0);
    }

    #[tokio::test]
    async fn unresolved_ref_block_txs_purged_on_head_advance() {
        let db = Database::memory();
        // Use a real chain so set_chain_head can drive `head_height`
        // forward beyond `VALIDITY_WINDOW`.
        let chain = linear_chain(&db, (VALIDITY_WINDOW as usize) + 5);
        let pool = InjectedTxMempool::with_capacity(db, 100);
        let pk = PrivateKey::random();

        // 100 txs each anchored at a random ref_block NOT in our DB.
        for salt in 0..100u8 {
            let bogus_ref_block = H256::random();
            pool.insert(signed_tx(&pk, ActorId::zero(), bogus_ref_block, salt).into())
                .await;
        }
        assert_eq!(pool.len().await, 100);

        // Advance head far past any tx's lifetime. Txs whose ref_block
        // never resolved are purged as `UnknownReferenceBlock` so the
        // public RPC `injected_send` can't permanently exhaust capacity.
        let head_idx = (VALIDITY_WINDOW as usize) + 1;
        let purged = pool.set_chain_head(chain[head_idx]).await;
        assert_eq!(pool.len().await, 0, "unresolved-ref_block txs must purge");
        assert_eq!(purged.len(), 100);
        assert!(
            purged
                .iter()
                .all(|p| matches!(p.reason, TransactionPurgedReason::UnknownReferenceBlock))
        );
    }

    #[tokio::test]
    async fn set_chain_head_purges_expired() {
        let db = Database::memory();
        // Build a chain long enough that `head_height -
        // VALIDITY_WINDOW` passes some block we'll insert against.
        let chain = linear_chain(&db, (VALIDITY_WINDOW as usize) + 5);
        let pool = InjectedTxMempool::new(db);

        let pk = PrivateKey::random();
        // tx anchored at block 1 — height 1
        let tx: Transaction = signed_tx(&pk, ActorId::zero(), chain[1].hash, 0).into();
        pool.insert(tx).await;
        assert_eq!(pool.len().await, 1);

        // Advance head far enough that block 1's height is past the
        // validity window. `is_expired` is `ref_height + WINDOW <= head_height`.
        let head_idx = (VALIDITY_WINDOW as usize) + 1;
        let _ = pool.set_chain_head(chain[head_idx]).await;
        assert_eq!(
            pool.len().await,
            0,
            "set_chain_head should purge txs whose ref_block aged out"
        );
    }

    #[tokio::test]
    async fn forget_moves_committed_to_seen_table() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::new(db);

        let pk = PrivateKey::random();
        let tx: Transaction = signed_tx(&pk, ActorId::zero(), chain[1].hash, 99).into();
        pool.insert(tx.clone()).await;
        assert_eq!(pool.len().await, 1);

        pool.forget(std::slice::from_ref(&tx.as_ref())).await;
        assert_eq!(pool.len().await, 0);

        // Re-inserting the same tx is a seen-hash no-op.
        assert_eq!(pool.insert(tx).await, TxInsertionStatus::AlreadyIncluded);
        assert_eq!(
            pool.len().await,
            0,
            "forgotten tx must not return to the pool"
        );
    }

    #[tokio::test]
    async fn fetch_filters_non_canonical_branches() {
        // Two branches diverging at block 1:
        //   genesis (hash[0]) -> b1 (hash[1])
        //                    \-> b1' (hash[1_alt])
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        // alt block off the same parent as chain[1]
        let alt_hash = H256::from([0xAA; 32]);
        let alt_header = BlockHeader {
            height: 1,
            timestamp: 1,
            parent_hash: chain[0].hash,
        };
        db.set_block_header(alt_hash, alt_header);
        db.mutate_block_meta(alt_hash, |_| {});

        // Globals' start_block_hash defaults to zero in `Database::memory`,
        // so the ancestor-walk fence won't trigger early. That's what we
        // want for this test.
        db.globals_mutate(|_| {});

        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();

        // tx anchored to the ALT branch
        let tx_alt: Transaction = signed_tx(&pk, ActorId::zero(), alt_hash, 1).into();
        pool.insert(tx_alt).await;
        assert_eq!(pool.len().await, 1);

        // Fetching for canonical branch (chain[1]) — alt tx must NOT
        // surface.
        let fetched = pool.fetch(chain[1]).await;
        assert!(
            fetched.is_empty(),
            "tx on alt branch must not be fetched against canonical head"
        );

        // Pool still holds it for a possible reorg.
        assert_eq!(pool.len().await, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn wait_for_new_tx_wakes_on_insert() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = std::sync::Arc::new(InjectedTxMempool::new(db));

        let waiter = {
            let pool = pool.clone();
            tokio::spawn(async move {
                pool.wait_for_new_tx().await;
            })
        };

        // Give the waiter a chance to register on the Notify.
        tokio::time::sleep(Duration::from_millis(10)).await;

        let pk = PrivateKey::random();
        pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 0).into())
            .await;

        // Waiter should now wake up promptly.
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("wait_for_new_tx must unblock after insert")
            .expect("waiter task panicked");
    }

    #[tokio::test(start_paused = true)]
    async fn wait_for_new_tx_does_not_wake_on_duplicate_insert() {
        // A duplicate insert returns Ok(()) but must not signal Notify —
        // the pool state didn't change, so there's nothing new for the
        // producer to fetch.
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = std::sync::Arc::new(InjectedTxMempool::new(db));
        let pk = PrivateKey::random();
        let tx: Transaction = signed_tx(&pk, ActorId::zero(), chain[1].hash, 0).into();

        // Seed one accepted insert and consume the resulting permit so
        // the next `.notified()` re-blocks until the next signal.
        pool.insert(tx.clone()).await;
        pool.wait_for_new_tx().await;

        let waiter = {
            let pool = pool.clone();
            tokio::spawn(async move {
                pool.wait_for_new_tx().await;
            })
        };

        tokio::time::sleep(Duration::from_millis(10)).await;

        // Same tx hash — idempotent no-op, no signal sent.
        pool.insert(tx).await;

        // Waiter must still be pending.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !waiter.is_finished(),
            "waiter must stay blocked when insert was a duplicate"
        );
        waiter.abort();
    }

    // ----------------------------------------------------------------
    // Property tests
    // ----------------------------------------------------------------
    //
    // The pool's contract is a small set of invariants that must hold
    // for arbitrary insert/forget/fetch orderings:
    //
    //   I1. `pool.len()` never exceeds `capacity`.
    //   I2. `forget` removes every committed tx (and the pool still
    //       respects (I1)).
    //   I3. `fetch(head, ...)` returns only txs whose `reference_block`
    //       is on the canonical ancestry of `head`.
    //   I4. After `forget(tx)`, re-inserting the same tx is a no-op
    //       (seen-hash dedup).
    //
    // Property tests below sample arbitrary insert/forget transcripts
    // and check the invariants hold at every step.

    use proptest::prelude::*;

    /// Proptest bodies are sync — drive the pool's async API with a
    /// local executor (tokio sync primitives are runtime-agnostic).
    fn bo<F: std::future::Future>(fut: F) -> F::Output {
        futures::executor::block_on(fut)
    }

    /// Build a deterministic linear chain in `db` and return the
    /// blocks oldest-first. `seed` makes hashes predictable across
    /// proptest cases (same input → same chain).
    fn linear_chain_seeded(db: &Database, len: usize, seed: u32) -> Vec<SimpleBlockData> {
        let mut chain = Vec::with_capacity(len);
        let mut parent = H256::zero();
        for i in 0..len {
            let mut hb = [0u8; 32];
            // Spread across the high bytes so different `seed`s never
            // alias each other within reasonable lengths.
            hb[0] = (seed & 0xff) as u8;
            hb[1] = ((seed >> 8) & 0xff) as u8;
            hb[2] = (i & 0xff) as u8;
            hb[3] = ((i >> 8) & 0xff) as u8;
            // Bias high so the hash is non-zero even if the seed is.
            hb[4] = 0x80;
            let hash = H256::from(hb);
            let header = BlockHeader {
                height: i as u32,
                timestamp: i as u64,
                parent_hash: parent,
            };
            db.set_block_header(hash, header);
            db.mutate_block_meta(hash, |_| {});
            chain.push(SimpleBlockData { hash, header });
            parent = hash;
        }
        chain
    }

    #[derive(Clone, Debug)]
    enum Action {
        Insert { ref_idx: usize, salt: u8 },
        Forget { which: usize },
    }

    fn arb_action(chain_len: usize) -> impl Strategy<Value = Action> {
        let insert = (0..chain_len, any::<u8>())
            .prop_map(|(ref_idx, salt)| Action::Insert { ref_idx, salt });
        let forget = (0..32usize).prop_map(|which| Action::Forget { which });
        prop_oneof![3 => insert, 1 => forget]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        /// Capacity is never exceeded regardless of the order of
        /// inserts or forgets.
        #[test]
        fn capacity_invariant_holds(
            actions in proptest::collection::vec(arb_action(8), 1..40),
            cap in 1usize..16,
            seed in any::<u32>(),
        ) {
            let db = Database::memory();
            let chain = linear_chain_seeded(&db, 8, seed);
            let pool = InjectedTxMempool::with_capacity(db.clone(), cap);
            let pk = PrivateKey::random();
            // Track inserted (and not-yet-forgotten) txs so Forget
            // can target a real entry.
            let mut live: Vec<Transaction> = Vec::new();
            for action in actions {
                match action {
                    Action::Insert { ref_idx, salt } => {
                        let tx: Transaction = signed_tx(&pk, ActorId::zero(), chain[ref_idx].hash, salt).into();
                        // Only track txs that actually entered the pool —
                        // `AlreadyInPool` / `AlreadyIncluded` / capacity
                        // rejects must not feed `live`, otherwise Forget
                        // would target a different occurrence.
                        if bo(pool.insert(tx.clone())) == TxInsertionStatus::Inserted {
                            live.push(tx);
                        }
                    }
                    Action::Forget { which } => {
                        if !live.is_empty() {
                            let idx = which % live.len();
                            let victim = live.swap_remove(idx);
                            bo(pool.forget(std::slice::from_ref(&victim.as_ref())));
                        }
                    }
                }
                // Capacity invariant — must hold after every step.
                prop_assert!(
                    bo(pool.len()) <= cap,
                    "pool.len()={} exceeded capacity {}",
                    bo(pool.len()),
                    cap
                );
            }
        }

        /// `fetch(head, _)` only returns txs whose `reference_block`
        /// is a canonical ancestor of `head`. Build a canonical
        /// chain plus an alt branch off block 0; insert txs against
        /// each; assert the alt-branch tx is NEVER returned for the
        /// canonical head.
        #[test]
        fn fetch_filters_alt_branch(
            n_txs in 1usize..8,
            seed in any::<u32>(),
        ) {
            let db = Database::memory();
            let chain = linear_chain_seeded(&db, 4, seed);
            // Alt block off block 0, distinct from chain[1].
            let alt_hash = {
                let mut hb = [0u8; 32];
                hb[0] = 0xAA;
                hb[1] = (seed & 0xff) as u8;
                H256::from(hb)
            };
            let alt_header = BlockHeader {
                height: 1,
                timestamp: 999,
                parent_hash: chain[0].hash,
            };
            db.set_block_header(alt_hash, alt_header);
            db.mutate_block_meta(alt_hash, |_| {});
            let pool = InjectedTxMempool::new(db);
            let pk = PrivateKey::random();

            // Inserts: alternating canonical-tail and alt anchors.
            for i in 0..n_txs {
                let anchor = if i % 2 == 0 { chain[3].hash } else { alt_hash };
                bo(pool.insert(signed_tx(&pk, ActorId::zero(), anchor, i as u8).into()));
            }

            let head = chain[3];
            let fetched = bo(pool.fetch(head));
            for tx in &fetched {
                prop_assert_ne!(
                    tx.as_injected()
                        .expect("injected transaction")
                        .data()
                        .reference_block,
                    alt_hash,
                    "alt-branch tx surfaced on canonical fetch"
                );
            }
        }

        /// After `forget(tx)`, re-inserting the same tx must be a
        /// no-op while its `reference_block` is still inside the
        /// validity window.
        #[test]
        fn forget_then_reinsert_is_noop(
            salt in any::<u8>(),
            seed in any::<u32>(),
        ) {
            let db = Database::memory();
            let chain = linear_chain_seeded(&db, 2, seed);
            let pool = InjectedTxMempool::new(db);
            let pk = PrivateKey::random();
            let tx: Transaction = signed_tx(&pk, ActorId::zero(), chain[1].hash, salt).into();
            bo(pool.insert(tx.clone()));
            prop_assert_eq!(bo(pool.len()), 1);
            bo(pool.forget(std::slice::from_ref(&tx.as_ref())));
            prop_assert_eq!(bo(pool.len()), 0);
            // Re-insert: idempotent no-op because the hash sits in
            // the seen-set and `reference_block` hasn't aged out.
            bo(pool.insert(tx));
            prop_assert_eq!(bo(pool.len()), 0);
        }
    }
}
