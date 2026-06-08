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
//!    - On insert we drop a tx only when its `ref_block` is known to
//!      the local DB AND already past the validity window. A
//!      not-yet-replicated `ref_block` is tolerated — the producer's
//!      EB lags the observer by O(seconds) in normal operation, and
//!      `fetch` already filters non-ancestors. To make this work
//!      during the cold-start window (before the observer's first
//!      `set_chain_head` tick) the head height is seeded from the
//!      DB's `latest_synced_eb` so the `is_expired` gate is active
//!      from process boot.
//!    - On fetch we return only txs whose `ref_block` is a canonical
//!      ancestor of the given `head`. Non-ancestors are kept — a
//!      reorg can make them eligible again.
//!    - On forget (finalized MB) we remove the tx from the pool and
//!      remember its hash in a seen-hash table. Subsequent inserts
//!      of the same tx are rejected. `purge_expired` only evicts a
//!      `seen` entry when its `ref_block` is known AND past the
//!      validity window — mirroring the insert tolerance so the
//!      dedup gate survives until the producer's EB catches up.
//!
//! The pool makes heavy use of `ethexe_db::Database::block_header` to
//! resolve `reference_block` into heights and to walk ancestor links;
//! all DB reads are synchronous and cheap (RocksDB point lookups).

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use ethexe_common::{
    HashOf, SimpleBlockData,
    db::{GlobalsStorageRO, InjectedStorageRW, OnChainStorageRO},
    injected::{
        InjectedTransaction, InjectedTransactionAcceptance, PurgedTransaction,
        SignedInjectedTransaction, TransactionPurgedReason, VALIDITY_WINDOW,
    },
};
use ethexe_db::Database;
use gprimitives::H256;
use tokio::sync::Notify;
use tracing::{info, trace};

/// Outcome of [`Mempool::insert`]. Splits into two groups:
///
/// - **Accept** — the tx is (now or already) tracked by this validator, so
///   the caller's promise subscription remains valid.
/// - **Reject** — the tx will never be processed by this validator and
///   the caller should treat it as terminal.
///
/// Group membership is queried via [`Self::is_accepted`]; the
/// `From<TxInsertionStatus> for InjectedTransactionAcceptance` impl uses
/// that to project into the RPC-facing acceptance type.
#[derive(Clone, Debug, PartialEq, Eq, derive_more::Display)]
pub enum TxInsertionStatus {
    // ---- Accept ----
    /// Fresh insert — the tx just entered the pool.
    #[display("inserted")]
    Inserted,
    /// Same tx hash already lives in the pool — idempotent no-op.
    #[display("already in pool")]
    AlreadyInPool,
    /// Same tx hash was committed within the validity window and is in
    /// the seen-hash table — idempotent no-op.
    #[display("already included within validity window")]
    AlreadyIncluded,
    // ---- Reject ----
    /// `reference_block` is past the validity window relative to the
    /// latest observed head.
    #[display("reference_block past validity window")]
    ExpiredRefBlock,
    /// Pool is at capacity.
    #[display("mempool at capacity")]
    PoolFull,
    /// Per #5083, non-zero-value injected transactions are not yet
    /// supported. Reject at insert so the pool never holds one — the
    /// proposer cannot accidentally select it and the runtime won't
    /// charge a panicking program for an out-of-budget transfer.
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

impl From<TxInsertionStatus> for InjectedTransactionAcceptance {
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
    /// Every pool-policy outcome — including the rejecting ones — is a
    /// [`TxInsertionStatus`] value. The method is infallible: invariant
    /// violations inside the implementation panic (e.g. a poisoned mutex)
    /// rather than surface as an error variant.
    fn insert(&self, tx: SignedInjectedTransaction) -> TxInsertionStatus;

    /// Drives validity-window GC.
    /// Returns the purged injected transactions.
    #[must_use]
    fn set_chain_head(&self, head: SimpleBlockData) -> Vec<PurgedTransaction>;

    /// Txs whose `reference_block` is an ancestor of `head`.
    async fn fetch(&self, head: SimpleBlockData) -> Vec<SignedInjectedTransaction>;

    /// Drop committed txs and remember their hashes for dedup.
    async fn forget(&self, committed: &[SignedInjectedTransaction]);

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
    fn insert(&self, _tx: SignedInjectedTransaction) -> TxInsertionStatus {
        TxInsertionStatus::Inserted
    }

    fn set_chain_head(&self, _head: SimpleBlockData) -> Vec<PurgedTransaction> {
        Vec::new()
    }

    async fn fetch(&self, _head: SimpleBlockData) -> Vec<SignedInjectedTransaction> {
        Vec::new()
    }

    async fn forget(&self, _committed: &[SignedInjectedTransaction]) {}

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

/// Pool entry — the signed tx plus the head height observed when it
/// was inserted. The head height anchors the grace window applied
/// when the tx's `reference_block` doesn't resolve via the local DB:
/// `purge_expired` keeps such an entry while
/// `head_height - inserted_at_head_height < VALIDITY_WINDOW` and
/// evicts it once that age is crossed.
#[derive(Debug)]
struct PoolEntry {
    tx: SignedInjectedTransaction,
    inserted_at_head_height: u32,
}

/// Seen entry — committed tx ref_block plus the head height observed
/// when `forget` ran. Mirrors [`PoolEntry`]'s grace-window policy
/// for the dedup table.
#[derive(Debug)]
struct SeenEntry {
    ref_block: H256,
    seen_at_head_height: u32,
}

/// Pool state behind a single mutex — operations are short, contention low.
#[derive(Debug, Default)]
struct Inner {
    pool: HashMap<HashOf<InjectedTransaction>, PoolEntry>,
    /// Recently committed txs (tx_hash → [`SeenEntry`]) for dedup. Aged out with the validity window.
    seen: HashMap<HashOf<InjectedTransaction>, SeenEntry>,
    /// Latest chain head height — drives age-out of pool/seen entries.
    /// Seeded from `db.globals().latest_synced_eb` at construction so
    /// the `is_expired` gate is active during the cold-start window.
    latest_head_height: Option<u32>,
}

#[derive(Debug)]
pub struct InjectedTxMempool {
    inner: Mutex<Inner>,
    db: Database,
    capacity: usize,
    /// Raised on insert; awaited by the producer in `wait_for_new_tx`.
    new_tx_notify: Arc<Notify>,
}

impl InjectedTxMempool {
    pub fn new(db: Database) -> Self {
        Self::with_capacity(db, DEFAULT_POOL_CAPACITY)
    }

    pub fn with_capacity(db: Database, capacity: usize) -> Self {
        // Seed `latest_head_height` from the DB's last-synced EB so the
        // `is_expired` gate in `insert` is active during the cold-start
        // window — between process boot and the observer's first
        // `set_chain_head` tick. Without this, fast-sync nodes accept
        // arbitrarily-old txs that the very next chain-head advance
        // would purge, misleading RPC clients with a hollow `Accept`.
        let initial_head_height = db.globals().latest_synced_eb.header.height;
        Self {
            inner: Mutex::new(Inner {
                latest_head_height: Some(initial_head_height),
                ..Default::default()
            }),
            db,
            capacity,
            new_tx_notify: Arc::new(Notify::new()),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("poisoned mempool").pool.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().expect("poisoned mempool").pool.is_empty()
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
    fn recent_ancestors(&self, head: &SimpleBlockData) -> HashSet<H256> {
        let start_fence = self.start_block_hash();

        let mut ancestors = HashSet::with_capacity(VALIDITY_WINDOW as usize + 1);
        ancestors.insert(head.hash);

        let mut current = head.hash;
        let mut parent = head.header.parent_hash;
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
    ///
    /// Eviction policy per entry:
    /// - `ref_block` known AND past validity window → evict (canonical
    ///   expiry).
    /// - `ref_block` unknown to the local DB AND the entry has lived
    ///   at least `VALIDITY_WINDOW` blocks since it was inserted
    ///   (grace window expired) → evict. Bounded back-pressure for
    ///   txs whose ref_block never lands or is bogus.
    /// - Otherwise → keep. Mirrors `insert`'s best-effort tolerance for
    ///   not-yet-replicated ref_blocks; without this, a lagging
    ///   observer would silently purge txs the local RPC just
    ///   `Accept`ed, and would break the `forget`→`seen` dedup gate
    ///   for committed txs whose ref_block hasn't replicated.
    fn purge_expired(inner: &mut Inner, head_height: u32, db: &Database) -> Vec<PurgedTransaction> {
        let mut purged_txs = Vec::new();
        inner.pool.retain(|tx_hash, entry| {
            let ref_block = entry.tx.data().reference_block;
            match db.block_header(ref_block).map(|h| h.height) {
                Some(h) if Self::is_expired(head_height, h) => {
                    trace!(
                        %tx_hash, %ref_block, ref_height = h, head_height,
                        "dropping expired tx from pool",
                    );
                    purged_txs.push(PurgedTransaction {
                        tx_hash: *tx_hash,
                        reason: TransactionPurgedReason::Outdated,
                    });
                    false
                }
                Some(_) => true,
                None if Self::grace_expired(head_height, entry.inserted_at_head_height) => {
                    trace!(
                        %tx_hash, %ref_block,
                        inserted_at_head_height = entry.inserted_at_head_height,
                        head_height,
                        "dropping tx with unresolved ref_block from pool — grace window expired",
                    );
                    purged_txs.push(PurgedTransaction {
                        tx_hash: *tx_hash,
                        reason: TransactionPurgedReason::UnknownReferenceBlock,
                    });
                    false
                }
                None => true,
            }
        });

        inner.seen.retain(|tx_hash, entry| {
            match db.block_header(entry.ref_block).map(|h| h.height) {
                Some(h) if Self::is_expired(head_height, h) => {
                    trace!(%tx_hash, ref_block = %entry.ref_block, "dropping expired seen-hash");
                    false
                }
                Some(_) => true,
                None if Self::grace_expired(head_height, entry.seen_at_head_height) => {
                    trace!(
                        %tx_hash,
                        ref_block = %entry.ref_block,
                        seen_at_head_height = entry.seen_at_head_height,
                        head_height,
                        "dropping seen-hash with unresolved ref_block — grace window expired",
                    );
                    false
                }
                None => true,
            }
        });
        purged_txs
    }

    /// Grace-window check for entries whose `reference_block` is not
    /// (yet) in the local DB. Mirrors [`Self::is_expired`]'s comparison
    /// shape against the entry's insertion-time head height.
    fn grace_expired(head_height: u32, inserted_at_head_height: u32) -> bool {
        head_height.saturating_sub(inserted_at_head_height) >= VALIDITY_WINDOW as u32
    }
}

#[async_trait]
impl Mempool for InjectedTxMempool {
    fn insert(&self, tx: SignedInjectedTransaction) -> TxInsertionStatus {
        let tx_data = tx.data();
        let tx_hash = tx_data.to_hash();
        let ref_block = tx_data.reference_block;

        // Reject non-zero-value txs unconditionally (#5083 — value-bearing
        // injected txs are not supported yet). Done first so a malicious
        // sender can't burn pool capacity with txs that will never be
        // selectable.
        if tx_data.value != 0 {
            info!(
                %tx_hash,
                value = tx_data.value,
                "mempool: rejecting tx — non-zero value (#5083 not supported)",
            );
            return TxInsertionStatus::NonZeroValue;
        }

        let inner = self.inner.lock().expect("poisoned mempool");

        if inner.seen.contains_key(&tx_hash) {
            info!(%tx_hash, "mempool: idempotent no-op — hash already committed within validity window");
            return TxInsertionStatus::AlreadyIncluded;
        }

        if inner.pool.contains_key(&tx_hash) {
            info!(%tx_hash, pool_len = inner.pool.len(), "mempool: idempotent no-op — duplicate insert");
            return TxInsertionStatus::AlreadyInPool;
        }

        // ref_block resolution is best-effort: a recipient that hasn't yet
        // observed the producer's reference Eth block accepts and filters
        // at fetch time once the block lands locally. Only reject when
        // the ref_block is known AND already past the validity window.
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

        if inner.pool.len() >= self.capacity {
            info!(%tx_hash, capacity = self.capacity, "mempool: rejecting tx — pool at capacity");
            return TxInsertionStatus::PoolFull;
        }

        // Drop the lock around the DB write so concurrent inserts /
        // fetches don't serialise behind disk I/O. After the write we
        // re-acquire and re-run the duplicate / capacity gates: another
        // concurrent insert could have landed the same tx hash, or
        // filled the last capacity slot, while we were writing.
        drop(inner);

        // TODO: #5489 remove, set in db only after mb finalization
        // Persist the tx so the local RPC's `injected_getTransactions`
        // can serve it to clients that look it up by hash later.
        // Done before inserting into the pool so a producer that
        // immediately picks the tx is guaranteed to find it in the DB.
        // The DB row is content-addressed by tx_hash, so two racing
        // writes converge on the same byte content.
        self.db.set_injected_transaction(tx.clone());

        let mut inner = self.inner.lock().expect("poisoned mempool");

        // Recheck dedup / capacity after the lock-free window.
        if inner.seen.contains_key(&tx_hash) {
            return TxInsertionStatus::AlreadyIncluded;
        }
        if inner.pool.contains_key(&tx_hash) {
            return TxInsertionStatus::AlreadyInPool;
        }
        if inner.pool.len() >= self.capacity {
            return TxInsertionStatus::PoolFull;
        }

        // Stamp the insertion head height so `purge_expired` can apply
        // a bounded grace window to entries whose `ref_block` never
        // resolves (lagging observer / bogus client input). Cold-start
        // gets the DB's `latest_synced_eb` height via `with_capacity`.
        let inserted_at_head_height = inner.latest_head_height.unwrap_or(0);
        let pool_len_after = inner.pool.len() + 1;
        inner.pool.insert(
            tx_hash,
            PoolEntry {
                tx,
                inserted_at_head_height,
            },
        );
        info!(
            %tx_hash,
            %ref_block,
            ref_height = ?ref_height_opt,
            pool_len = pool_len_after,
            "mempool: insert accepted",
        );

        // Drop the lock before signaling so a waiter resumed
        // immediately doesn't have to bounce on the mutex.
        drop(inner);
        self.new_tx_notify.notify_one();
        TxInsertionStatus::Inserted
    }

    fn set_chain_head(&self, head: SimpleBlockData) -> Vec<PurgedTransaction> {
        let mut inner = self.inner.lock().expect("poisoned mempool");
        let h = head.header.height;
        if inner.latest_head_height == Some(h) {
            // Same height re-sent — nothing to GC beyond what we
            // already did on the previous call.
            return Default::default();
        }
        inner.latest_head_height = Some(h);
        Self::purge_expired(&mut inner, h, &self.db)
    }

    async fn fetch(&self, head: SimpleBlockData) -> Vec<SignedInjectedTransaction> {
        let ancestors = self.recent_ancestors(&head);

        let inner = self.inner.lock().expect("poisoned mempool");
        let pool_len = inner.pool.len();
        let result: Vec<_> = inner
            .pool
            .values()
            .filter(|entry| ancestors.contains(&entry.tx.data().reference_block))
            .map(|entry| entry.tx.clone())
            .collect();
        info!(
            head_hash = %head.hash,
            head_height = head.header.height,
            ancestors = ancestors.len(),
            pool_len,
            returned = result.len(),
            "mempool: fetch",
        );
        result
    }

    async fn forget(&self, committed: &[SignedInjectedTransaction]) {
        let mut inner = self.inner.lock().expect("poisoned mempool");
        let seen_at_head_height = inner.latest_head_height.unwrap_or(0);
        for tx in committed {
            let tx_hash = tx.data().to_hash();
            inner.pool.remove(&tx_hash);
            inner.seen.insert(
                tx_hash,
                SeenEntry {
                    ref_block: tx.data().reference_block,
                    seen_at_head_height,
                },
            );
        }
    }

    async fn wait_for_new_tx(&self) {
        // The insert path uses `notify_one`, which preserves one
        // pending permit when no waiter is parked. So a tx that
        // lands between the producer's `fetch()` and its `.notified()`
        // call still wakes the next `.notified()` immediately.
        // The caller must still re-check `fetch()` after returning —
        // a permit consumed here may correspond to a tx the next
        // `fetch()` already covered, in which case we just loop and
        // wait again.
        self.new_tx_notify.notified().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        BlockHeader, PrivateKey, SignedMessage, SimpleBlockData,
        db::{BlockMetaStorageRW, GlobalsStorageRW, OnChainStorageRW},
        injected::{InjectedTransaction, InjectedTransactionAcceptance},
    };
    use gprimitives::ActorId;
    use std::time::Duration;

    /// Pins the `TxInsertionStatus -> InjectedTransactionAcceptance` split.
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
                InjectedTransactionAcceptance::from(status),
                InjectedTransactionAcceptance::Accept,
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
                InjectedTransactionAcceptance::from(status),
                InjectedTransactionAcceptance::Reject { reason },
            );
        }
    }

    #[test]
    fn insert_rejects_non_zero_value_before_pool_state_checks() {
        // Verifies NonZeroValue fires *before* the pool is consulted —
        // a tx with value != 0 must never reach the seen / duplicate /
        // capacity gates. We seed the pool to capacity first to make
        // sure those gates would fire if reached.
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::with_capacity(db, 1);
        let pk = PrivateKey::random();

        // Fill to capacity with a valid tx so PoolFull would normally fire.
        pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 0));

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

        assert_eq!(pool.insert(value_tx), TxInsertionStatus::NonZeroValue,);
        assert_eq!(pool.len(), 1, "non-zero-value tx must not enter the pool");
    }

    /// Fresh insert that passes every gate must return `Inserted`.
    #[test]
    fn insert_returns_inserted_for_fresh_tx() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();
        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 0);

        assert_eq!(pool.insert(tx), TxInsertionStatus::Inserted);
        assert_eq!(pool.len(), 1);
    }

    /// Same tx inserted twice — second insert hits the pool table and
    /// returns `AlreadyInPool` without bumping the size.
    #[test]
    fn insert_returns_already_in_pool_for_duplicate() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();
        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 5);

        assert_eq!(pool.insert(tx.clone()), TxInsertionStatus::Inserted);
        assert_eq!(pool.insert(tx), TxInsertionStatus::AlreadyInPool,);
        assert_eq!(pool.len(), 1);
    }

    /// After `forget`, re-inserting the same tx hits the seen-hash table
    /// and returns `AlreadyIncluded`.
    #[test]
    fn insert_returns_already_included_for_committed_tx() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();
        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 11);

        pool.insert(tx.clone());
        futures::executor::block_on(pool.forget(std::slice::from_ref(&tx)));
        assert_eq!(pool.len(), 0);

        assert_eq!(pool.insert(tx), TxInsertionStatus::AlreadyIncluded,);
        assert_eq!(pool.len(), 0);
    }

    /// `ExpiredRefBlock` fires once `set_chain_head` has advanced past
    /// `ref_block_height + VALIDITY_WINDOW` and the tx is brand new.
    #[test]
    fn insert_returns_expired_ref_block() {
        let db = Database::memory();
        let chain = linear_chain(&db, (VALIDITY_WINDOW as usize) + 5);
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();

        // Advance head so block 1 is past the validity window.
        let head_idx = (VALIDITY_WINDOW as usize) + 1;
        let _ = pool.set_chain_head(chain[head_idx]);

        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 0);
        assert_eq!(pool.insert(tx), TxInsertionStatus::ExpiredRefBlock,);
        assert_eq!(pool.len(), 0);
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

    #[test]
    fn insert_unknown_ref_block_is_accepted() {
        let db = Database::memory();
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();
        let tx = signed_tx(&pk, ActorId::zero(), H256::random(), 1);
        pool.insert(tx);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn insert_then_fetch_round_trip() {
        let db = Database::memory();
        let chain = linear_chain(&db, 3);
        let pool = InjectedTxMempool::new(db);

        let pk = PrivateKey::random();
        let tx = signed_tx(&pk, ActorId::zero(), chain[2].hash, 1);
        let tx_hash = tx.data().to_hash();

        pool.insert(tx.clone());
        assert_eq!(pool.len(), 1);

        // The pool fetches when ref_block is on the canonical chain
        // of the head we hand it.
        let head = chain[2];
        let fetched = futures::executor::block_on(pool.fetch(head));
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].data().to_hash(), tx_hash);
    }

    #[test]
    fn capacity_limit_blocks_further_inserts() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::with_capacity(db, 2);

        let pk = PrivateKey::random();
        pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 0));
        pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 1));
        assert_eq!(
            pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 2)),
            TxInsertionStatus::PoolFull,
        );
        assert_eq!(pool.len(), 2, "third insert must hit the capacity cap");
    }

    #[test]
    fn pool_retains_unresolved_ref_block_indefinitely() {
        let db = Database::memory();
        // Use a real chain so set_chain_head can drive `head_height`
        // forward beyond `VALIDITY_WINDOW`.
        let chain = linear_chain(&db, (VALIDITY_WINDOW as usize) + 5);
        let pool = InjectedTxMempool::with_capacity(db, 100);
        let pk = PrivateKey::random();

        // 100 txs each anchored at a random ref_block NOT in our DB.
        for salt in 0..100u8 {
            let bogus_ref_block = H256::random();
            pool.insert(signed_tx(&pk, ActorId::zero(), bogus_ref_block, salt));
        }
        assert_eq!(pool.len(), 100);

        // Advance head far past any tx's lifetime.
        let head_idx = (VALIDITY_WINDOW as usize) + 1;
        let _ = pool.set_chain_head(chain[head_idx]);

        // Desired behaviour: txs whose ref_block never resolved AND
        // whose insert is older than VALIDITY_WINDOW should be evicted
        // (mirroring the `seen` retain policy). Currently they all
        // stay — capacity is permanently exhausted.
        assert!(
            pool.len() < 100,
            "pool retains all {} unresolved-ref_block txs after head advanced past WINDOW — \
             public RPC `injected_send` can permanently exhaust capacity",
            pool.len(),
        );
    }

    #[test]
    fn set_chain_head_purges_expired() {
        let db = Database::memory();
        // Build a chain long enough that `head_height -
        // VALIDITY_WINDOW` passes some block we'll insert against.
        let chain = linear_chain(&db, (VALIDITY_WINDOW as usize) + 5);
        let pool = InjectedTxMempool::new(db);

        let pk = PrivateKey::random();
        // tx anchored at block 1 — height 1
        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 0);
        pool.insert(tx);
        assert_eq!(pool.len(), 1);

        // Advance head far enough that block 1's height is past the
        // validity window. `is_expired` is `ref_height + WINDOW <= head_height`.
        let head_idx = (VALIDITY_WINDOW as usize) + 1;
        let _ = pool.set_chain_head(chain[head_idx]);
        assert_eq!(
            pool.len(),
            0,
            "set_chain_head should purge txs whose ref_block aged out"
        );
    }

    /// Regression for the cold-start expiry bypass: before the fix the
    /// `is_expired` gate in `insert` was guarded on
    /// `latest_head_height.is_some()`, so any insert that landed in the
    /// window between process boot and the observer's first
    /// `set_chain_head` tick skipped the expiry check entirely. RPC
    /// returned `Accept` for arbitrarily-old txs that the very next
    /// chain-head advance silently purged. The fix seeds
    /// `latest_head_height` from `db.globals().latest_synced_eb` at
    /// construction so the gate is active from boot.
    #[test]
    fn cold_start_insert_rejects_expired_ref_block_using_latest_synced_eb() {
        let db = Database::memory();
        // Post-fast-sync state: a long chain in `block_header` and a
        // `latest_synced_eb` pointer set by the observer at some prior
        // run. The current process has NOT yet ticked `set_chain_head`.
        let chain = linear_chain(&db, (VALIDITY_WINDOW as usize) + 5);
        let last = chain.last().expect("chain non-empty");
        db.globals_mutate(|g| g.latest_synced_eb = *last);

        let pool = InjectedTxMempool::with_capacity(db, 4);
        let pk = PrivateKey::random();

        // tx anchored at block 1 (height 1). Tip is at height
        // `VALIDITY_WINDOW + 4`, so `1 + WINDOW <= tip_height` —
        // expired by any sane head proxy.
        let expired_tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 0);
        assert_eq!(
            pool.insert(expired_tx),
            TxInsertionStatus::ExpiredRefBlock,
            "cold-start insert must apply `is_expired` using \
             `latest_synced_eb` as the head proxy when the observer \
             has not yet ticked — otherwise public RPC returns Accept \
             for txs that the first `set_chain_head` would purge",
        );
        assert_eq!(pool.len(), 0);
    }

    /// Regression for the insert→purge race on a lagging observer: the
    /// `insert` path tolerates not-yet-replicated `ref_block`s (the
    /// producer's EB lags the observer by O(seconds)). Before the
    /// fix, the very next `set_chain_head` ran `purge_expired` which
    /// evicted the tx on the `_ => false` arm — orphaning the
    /// promise the local RPC just `Accept`ed. The fix keeps such
    /// entries within a `VALIDITY_WINDOW`-block grace period.
    #[test]
    fn purge_expired_keeps_tx_with_unresolved_ref_block_within_grace() {
        let db = Database::memory();
        let chain = linear_chain(&db, 3);
        let pool = InjectedTxMempool::with_capacity(db, 8);
        let pk = PrivateKey::random();

        // Simulate the lag: client posts a tx anchored at a ref_block
        // the producer knows but our observer hasn't synced yet.
        let unsynced_ref_block = H256::from([0xCA; 32]);
        let tx = signed_tx(&pk, ActorId::zero(), unsynced_ref_block, 0);
        assert!(
            pool.insert(tx).is_accepted(),
            "insert tolerates unknown ref_block",
        );
        assert_eq!(pool.len(), 1);

        // The next chain-head advance triggers `purge_expired`. The
        // ref_block is still unknown — that's the exact race the
        // grace window covers.
        let _ = pool.set_chain_head(chain[1]);
        assert_eq!(
            pool.len(),
            1,
            "tx with not-yet-replicated ref_block must survive the \
             very next set_chain_head — grace window of VALIDITY_WINDOW \
             blocks. The producer's EB normally lands within seconds.",
        );
    }

    /// Regression for the forget→purge dedup bypass: `forget()`
    /// stamps every committed tx into `seen`. Before the fix, if the
    /// committed tx's `ref_block` hadn't yet replicated to this
    /// validator's DB, the next `set_chain_head` evicted the `seen`
    /// entry via `_ => false` — letting the same network-committed
    /// tx re-enter the local pool. The grace-window fix retains the
    /// `seen` entry for `VALIDITY_WINDOW` blocks past `forget` even
    /// when the ref_block is unknown.
    #[test]
    fn forget_then_purge_keeps_seen_for_unresolved_ref_block_within_grace() {
        let db = Database::memory();
        let chain = linear_chain(&db, 3);
        let pool = InjectedTxMempool::with_capacity(db, 8);
        let pk = PrivateKey::random();

        // Network committed this tx; our observer hasn't synced its
        // ref_block yet.
        let unsynced_ref_block = H256::from([0xCA; 32]);
        let tx = signed_tx(&pk, ActorId::zero(), unsynced_ref_block, 0);

        // process_finalized → forget() for a tx we never pooled
        // locally. The `seen` table stamps tx_hash → (ref_block, head=0).
        futures::executor::block_on(pool.forget(std::slice::from_ref(&tx)));

        // Sanity: dedup gate is active immediately after forget.
        assert_eq!(
            pool.insert(tx.clone()),
            TxInsertionStatus::AlreadyIncluded,
        );

        // Next chain-head advance fires `purge_expired`. With the
        // grace-window fix the seen entry survives — dedup gate
        // intact.
        let _ = pool.set_chain_head(chain[1]);
        assert_eq!(
            pool.insert(tx),
            TxInsertionStatus::AlreadyIncluded,
            "forgotten tx with not-yet-replicated ref_block must remain \
             in `seen` across the next set_chain_head — otherwise a \
             re-submitted committed tx slips back into the local pool",
        );
    }

    #[test]
    fn forget_moves_committed_to_seen_table() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::new(db);

        let pk = PrivateKey::random();
        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 99);
        pool.insert(tx.clone());
        assert_eq!(pool.len(), 1);

        futures::executor::block_on(pool.forget(std::slice::from_ref(&tx)));
        assert_eq!(pool.len(), 0);

        // Re-inserting the same tx is a seen-hash no-op.
        assert_eq!(pool.insert(tx), TxInsertionStatus::AlreadyIncluded);
        assert_eq!(pool.len(), 0, "forgotten tx must not return to the pool");
    }

    #[test]
    fn fetch_filters_non_canonical_branches() {
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
        let tx_alt = signed_tx(&pk, ActorId::zero(), alt_hash, 1);
        pool.insert(tx_alt);
        assert_eq!(pool.len(), 1);

        // Fetching for canonical branch (chain[1]) — alt tx must NOT
        // surface.
        let fetched = futures::executor::block_on(pool.fetch(chain[1]));
        assert!(
            fetched.is_empty(),
            "tx on alt branch must not be fetched against canonical head"
        );

        // Pool still holds it for a possible reorg.
        assert_eq!(pool.len(), 1);
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
        pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 0));

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
        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 0);

        // Seed one accepted insert and consume the resulting permit so
        // the next `.notified()` re-blocks until the next signal.
        pool.insert(tx.clone());
        pool.wait_for_new_tx().await;

        let waiter = {
            let pool = pool.clone();
            tokio::spawn(async move {
                pool.wait_for_new_tx().await;
            })
        };

        tokio::time::sleep(Duration::from_millis(10)).await;

        // Same tx hash — idempotent no-op, no signal sent.
        pool.insert(tx);

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
            let mut live: Vec<SignedInjectedTransaction> = Vec::new();
            for action in actions {
                match action {
                    Action::Insert { ref_idx, salt } => {
                        let tx = signed_tx(&pk, ActorId::zero(), chain[ref_idx].hash, salt);
                        // Only track txs that actually entered the pool —
                        // `AlreadyInPool` / `AlreadyIncluded` / capacity
                        // rejects must not feed `live`, otherwise Forget
                        // would target a different occurrence.
                        if pool.insert(tx.clone()) == TxInsertionStatus::Inserted {
                            live.push(tx);
                        }
                    }
                    Action::Forget { which } => {
                        if !live.is_empty() {
                            let idx = which % live.len();
                            let victim = live.swap_remove(idx);
                            futures::executor::block_on(pool.forget(std::slice::from_ref(&victim)));
                        }
                    }
                }
                // Capacity invariant — must hold after every step.
                prop_assert!(
                    pool.len() <= cap,
                    "pool.len()={} exceeded capacity {}",
                    pool.len(),
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
                pool.insert(signed_tx(&pk, ActorId::zero(), anchor, i as u8));
            }

            let head = chain[3];
            let fetched = futures::executor::block_on(pool.fetch(head));
            for tx in &fetched {
                prop_assert_ne!(
                    tx.data().reference_block, alt_hash,
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
            let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, salt);
            pool.insert(tx.clone());
            prop_assert_eq!(pool.len(), 1);
            futures::executor::block_on(pool.forget(std::slice::from_ref(&tx)));
            prop_assert_eq!(pool.len(), 0);
            // Re-insert: idempotent no-op because the hash sits in
            // the seen-set and `reference_block` hasn't aged out.
            pool.insert(tx);
            prop_assert_eq!(pool.len(), 0);
        }
    }
}
