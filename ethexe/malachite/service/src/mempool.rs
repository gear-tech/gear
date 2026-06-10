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

/// Pool state behind a single mutex — operations are short, contention low.
#[derive(Debug, Default)]
struct Inner {
    pool: HashMap<HashOf<InjectedTransaction>, SignedInjectedTransaction>,
    /// Recently committed txs (tx_hash → ref_block) for dedup. Aged out with the validity window.
    seen: HashMap<HashOf<InjectedTransaction>, H256>,
    /// Latest chain head height — drives age-out of pool/seen entries.
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
        Self {
            inner: Mutex::new(Inner::default()),
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
    fn purge_expired(inner: &mut Inner, head_height: u32, db: &Database) -> Vec<PurgedTransaction> {
        inner.seen.retain(|tx_hash, ref_block| {
            match db.block_header(*ref_block).map(|h| h.height) {
                Some(h) if !Self::is_expired(head_height, h) => true,
                _ => {
                    trace!(%tx_hash, ref_block = %ref_block, "dropping expired seen-hash");
                    false
                }
            }
        });
        let mut purged_txs = Vec::new();
        inner.pool.retain(|tx_hash, tx| {
            let ref_block = tx.data().reference_block;
            match db.block_header(ref_block).map(|h| h.height) {
                Some(h) if !Self::is_expired(head_height, h) => true,
                Some(h) => {
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
                None => {
                    trace!(
                        %tx_hash, %ref_block,
                        "dropping tx with unknown ref_block from pool",
                    );
                    purged_txs.push(PurgedTransaction {
                        tx_hash: *tx_hash,
                        reason: TransactionPurgedReason::UnknownReferenceBlock,
                    });
                    false
                }
            }
        });
        purged_txs
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

        let pool_len_after = inner.pool.len() + 1;
        inner.pool.insert(tx_hash, tx);
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
            .filter(|tx| ancestors.contains(&tx.data().reference_block))
            .cloned()
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
        for tx in committed {
            let tx_hash = tx.data().to_hash();
            inner.pool.remove(&tx_hash);
            inner.seen.insert(tx_hash, tx.data().reference_block);
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

    /// REPRODUCES: the mempool accepts txs whose `reference_block` is at a
    /// height STRICTLY GREATER than the current chain head ("future
    /// reference"). Such txs are NEVER fetchable (the future block is not
    /// on `recent_ancestors` of any present head) and `tx_validity.rs`
    /// (`is_reference_block_within_validity_window`) explicitly rejects
    /// `reference_block_height > chain_head_height`. They are also not
    /// purged by `set_chain_head` until the head catches up past their
    /// height + VALIDITY_WINDOW. That window can be made arbitrarily wide
    /// — a malicious caller can mint a payload to permanently exhaust pool
    /// capacity with txs that no producer will ever include.
    ///
    /// The desired behaviour is that the mempool refuses to accept
    /// future-anchored refs (aligning with `tx_validity.rs:184`). The
    /// final assertion below pins that invariant; the test currently
    /// fails because mempool accepts the legitimate fresh tx after the
    /// poisoned ones (capacity is not actually full — because the future
    /// txs are unfetchable but still occupy slots — the inserts succeed
    /// when they should have been rejected outright).
    #[test]
    #[ignore = "tracks bug: mempool accepts future-anchored ref_block but tx_validity rejects it — capacity DoS"]
    fn insert_should_reject_future_ref_block() {
        let db = Database::memory();
        // Build a short canonical chain so head_height is meaningful.
        let chain = linear_chain(&db, 3);
        // Inject a "future" block header at height 100 — well above the
        // head_height of 2 that we will set. This simulates a block the
        // observer wrote (any branch ever seen lands in DB) that hasn't
        // been promoted to head locally.
        let future_hash = H256::from([0xFE; 32]);
        let future_header = BlockHeader {
            height: 100,
            timestamp: 100,
            parent_hash: chain[2].hash,
        };
        db.set_block_header(future_hash, future_header);
        db.mutate_block_meta(future_hash, |_| {});

        let pool = InjectedTxMempool::with_capacity(db, 4);
        pool.set_chain_head(chain[2]); // head_height = 2

        let pk = PrivateKey::random();
        // A tx anchored to a FUTURE block (height 100 > head_height 2).
        // Desired: rejected. Actual (bug): accepted.
        let future_tx = signed_tx(&pk, ActorId::zero(), future_hash, 0);
        let insert_result = pool.insert(future_tx);
        assert!(
            matches!(insert_result, Err(MempoolInsertError::ExpiredRefBlock)),
            "tx with reference_block_height ({}) > chain_head_height ({}) \
             must be rejected at insert to match tx_validity.rs:184 \
             (`reference_block_height <= chain_head_height`); got {:?}",
            100,
            2,
            insert_result,
        );
    }

    /// REPRODUCES: `insert` deliberately tolerates a `reference_block`
    /// that hasn't yet been observed locally (see the comment at
    /// mempool.rs:298-301: "ref_block resolution is best-effort: a
    /// recipient that hasn't yet observed the producer's reference Eth
    /// block accepts and filters at fetch time once the block lands
    /// locally").
    ///
    /// But `purge_expired` — invoked by `set_chain_head` on every
    /// height advance — treats `db.block_header(ref_block) == None` as
    /// "drop this tx". So the very next time the local node receives a
    /// block, every pool entry whose ref_block hasn't yet replicated
    /// is silently evicted, even though the network as a whole has it
    /// and would have produced the block in a few hundred ms.
    ///
    /// Concrete attack/race: validator A publishes its `BlockSynced`
    /// for an EB at the same instant validator B fans out an injected
    /// tx whose ref_block is that EB. If B's RPC reaches A a tick
    /// before A's observer writes the EB header, A accepts the tx
    /// (insert tolerates unknown ref_block). The next `set_chain_head`
    /// on A (very next EB) purges the tx — but A's RPC had already
    /// returned `Accept` to the client, and the promise will never
    /// fire because the tx is gone before any producer fetched it.
    ///
    /// Desired behaviour (one of two fixes):
    ///   (a) `purge_expired` keeps unknown-ref_block entries that
    ///       arrived within a short grace window of `latest_head_height`
    ///       (mirroring the insert tolerance), OR
    ///   (b) `insert` rejects unknown ref_block when a `chain_head` is
    ///       already set, so RPC's `Accept` matches the runtime fate.
    ///
    /// This test asserts (a): an unknown-ref_block tx accepted by
    /// `insert` must survive the next `set_chain_head` for at least
    /// one block. It currently fails because the tx is dropped
    /// immediately.
    #[test]
    #[ignore = "tracks bug: purge_expired drops unknown-ref_block txs that insert just accepted"]
    fn purge_expired_must_not_evict_unknown_ref_block_within_grace() {
        let db = Database::memory();
        // Canonical chain so set_chain_head has a real head to consume.
        let chain = linear_chain(&db, 3);
        let pool = InjectedTxMempool::with_capacity(db, 8);
        let pk = PrivateKey::random();

        // Simulate the race: the producer's ref_block hasn't replicated
        // to this validator's DB yet. Use a random hash that's NOT in
        // the DB. insert tolerates this and accepts.
        let unsynced_ref_block = H256::from([0xCA; 32]);
        let tx = signed_tx(&pk, ActorId::zero(), unsynced_ref_block, 0);
        pool.insert(tx).expect("insert tolerates unknown ref_block");
        assert_eq!(pool.len(), 1, "insert path accepted the tx");

        // The very next chain-head advance triggers purge_expired.
        // The tx's ref_block is still unknown in the local DB — but
        // that's the EXACT race the insert tolerance is meant to
        // cover. The producer-side EB will replicate to this node a
        // few hundred ms later, and at that point the tx should still
        // be fetchable.
        pool.set_chain_head(chain[1]);

        assert_eq!(
            pool.len(),
            1,
            "tx with not-yet-replicated ref_block must survive \
             set_chain_head for at least one block — insert tolerates \
             unknown ref_block, so purge_expired must mirror that \
             tolerance (else RPC returns Accept but the promise never \
             fires)",
        );
    }

    /// REPRODUCES: `forget()` unconditionally stamps every committed
    /// tx into the `seen` table with its `reference_block` hash. When
    /// `ref_block` isn't (yet) in this node's local DB,
    /// `purge_expired` — fired on every `set_chain_head` — evicts the
    /// seen entry because the `db.block_header(ref_block)` lookup
    /// returns `None` (see `Self::purge_expired`'s seen-retain loop:
    /// match arm `_ => false`). Once the seen entry is gone, the
    /// network-committed tx can be re-inserted into the local pool —
    /// the dedup guarantee `forget_moves_committed_to_seen_table`
    /// relies on is silently broken in this race.
    ///
    /// The race is realistic: the proposer's MB references an EB the
    /// validator hasn't yet observed via the observer stream
    /// (the insert path explicitly tolerates this in
    /// `mempool.rs:298-301`). `process_finalized` calls `forget()`
    /// for every tx in the committed MB — including ones the local
    /// node never saw because its EB stream lags. Those forgotten
    /// txs are then evicted from `seen` on the next chain-head
    /// advance.
    ///
    /// Concrete consequence: a client can re-submit the SAME signed
    /// tx after it was already committed by the network, and this
    /// node will admit it into its pool a second time. If this node
    /// later becomes proposer, it would include the duplicate —
    /// `TxValidityChecker::recent_included_txs` covers only the last
    /// `VALIDITY_WINDOW` MBs, so a sufficiently lagged ref_block plus
    /// a deeply committed earlier tx slip through. Even before that,
    /// it inflates pool occupancy with already-committed work.
    ///
    /// Expected fix: `purge_expired` must retain `seen` entries
    /// whose ref_block isn't yet in the DB — same grace the insert
    /// path extends to incoming txs. The eviction rule should be
    /// "known AND expired", not "known AND expired OR unknown".
    /// (Symmetric to iter #4 but on the forget→purge path, not the
    /// insert→purge path.)
    #[test]
    #[ignore = "tracks bug: purge_expired evicts seen-table entries whose ref_block hasn't replicated yet"]
    fn forget_then_purge_evicts_seen_entry_for_unknown_ref_block() {
        let db = Database::memory();
        // Canonical chain so set_chain_head has a real head to consume.
        let chain = linear_chain(&db, 3);
        let pool = InjectedTxMempool::with_capacity(db, 8);
        let pk = PrivateKey::random();

        // The committed tx references an EB that this validator hasn't
        // yet observed — its ref_block hash is NOT in the local DB.
        // process_finalized calls forget() with this tx anyway: the
        // tx was committed by the network, and the local node accepts
        // the commit even when its observer stream lags.
        let unsynced_ref_block = H256::from([0xCA; 32]);
        let tx = signed_tx(&pk, ActorId::zero(), unsynced_ref_block, 0);

        // Simulate process_finalized → forget() for a tx that was
        // never in our local pool. forget() unconditionally stamps
        // the tx_hash into `seen` with its ref_block.
        futures::executor::block_on(pool.forget(std::slice::from_ref(&tx)));

        // Sanity: re-inserting the just-forgotten tx is blocked by the
        // seen-hash gate.
        assert!(
            matches!(
                pool.insert(tx.clone()),
                Err(MempoolInsertError::AlreadyCommitted),
            ),
            "seen-hash gate must block re-insert of a just-forgotten tx",
        );

        // The next chain-head advance triggers purge_expired. Its
        // seen-retain loop falls through to `_ => false` for the
        // unknown ref_block — and silently drops the seen entry.
        pool.set_chain_head(chain[1]);

        // The bug: the dedup gate is gone. The same network-committed
        // tx now slips back into the local pool.
        let reinsert = pool.insert(tx);
        assert!(
            matches!(reinsert, Err(MempoolInsertError::AlreadyCommitted)),
            "forgotten tx with not-yet-replicated ref_block must remain \
             in the `seen` table across set_chain_head — purge_expired \
             must mirror insert's tolerance for unknown ref_block. \
             Currently re-insert returns: {reinsert:?}",
        );
    }

    /// REPRODUCES: `insert` gates the `is_expired` check on
    /// `latest_head_height.is_some()`. Before the first `set_chain_head`
    /// arrives — the node's "cold start" window between process boot
    /// and the first observer tick — the check is silently skipped.
    /// During fast-sync the local DB already holds a long chain of
    /// `block_header` rows (so `ref_block_height` resolves), but
    /// `set_chain_head` hasn't fired yet because the observer hasn't
    /// produced its first event. In this window, a public RPC caller
    /// can submit txs anchored on arbitrarily-old ref_blocks (well
    /// past `VALIDITY_WINDOW`) and the pool accepts them.
    ///
    /// Concrete consequence: RPC returns `Accept` to the client, the
    /// tx occupies a pool slot, and the first `set_chain_head` call
    /// then evicts it via `purge_expired` — the promise the client is
    /// waiting on never resolves. An attacker who races the cold-start
    /// window can DoS legitimate clients by burning capacity slots
    /// AND by tricking the local RPC into returning misleading
    /// acceptances. Distinct from iter #2 (future-anchored ref_block,
    /// chain_head SET) and iter #4 (unknown ref_block, insert tolerance
    /// vs purge mismatch).
    ///
    /// Expected fix: when `latest_head_height` is `None` but the
    /// `ref_block` IS in the local DB, derive the expiry from a
    /// canonical-head proxy (e.g. the DB's `latest_synced_eb` or the
    /// max known block_header height), so cold-start inserts use the
    /// same expiry rule as steady-state inserts.
    #[test]
    #[ignore = "tracks bug: cold-start mempool insert skips is_expired when latest_head_height is None"]
    fn cold_start_insert_accepts_expired_ref_block_before_first_set_chain_head() {
        let db = Database::memory();
        // A long chain in the DB — mirrors the post-fast-sync state at
        // boot, BEFORE the observer has produced its first event.
        let chain = linear_chain(&db, (VALIDITY_WINDOW as usize) + 5);
        let pool = InjectedTxMempool::with_capacity(db, 4);
        let pk = PrivateKey::random();

        // No `pool.set_chain_head(..)` call — simulate cold start.

        // A tx anchored at block 1 — height 1. The actual chain tip
        // (chain[VALIDITY_WINDOW + 4]) is well past the validity window
        // for block 1, so this tx would be expired against any sane
        // canonical head. Insert MUST reject it; current behaviour
        // accepts it because `latest_head_height` is None.
        let expired_tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 0);
        let insert_result = pool.insert(expired_tx);

        assert!(
            matches!(insert_result, Err(MempoolInsertError::ExpiredRefBlock)),
            "cold-start insert accepted an expired-against-DB-tip tx \
             (ref_block height 1; DB has blocks up to height {}). The \
             pool must apply the same `is_expired` rule when \
             `latest_head_height` is None — otherwise public RPC \
             returns Accept to clients for txs that the very next \
             `set_chain_head` will silently purge. Got: {:?}",
            (VALIDITY_WINDOW as usize) + 4,
            insert_result,
        );
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
