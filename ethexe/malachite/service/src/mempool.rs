// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Source of injected transactions for the Malachite producer.
//!
//! Two layers in this module:
//!
//! 1. The [`Mempool`] trait — abstract dependency consumed by
//!    [`crate::EthexeExternalities`] when [`ethexe_malachite_core::Externalities::build_block_above`]
//!    fires. Tests can stub it with [`EmptyMempool`]; production
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
        InjectedTransaction, PurgedTransaction, SignedInjectedTransaction, TransactionPurgedReason,
        VALIDITY_WINDOW,
    },
};
use ethexe_db::Database;
use gprimitives::H256;
use tokio::sync::Notify;
use tracing::{info, trace};

/// Reasons a tx can be rejected at insert time.
#[derive(Debug, thiserror::Error)]
pub enum MempoolInsertError {
    #[error("tx hash already committed within validity window")]
    AlreadyCommitted,
    #[error("tx already in pool")]
    Duplicate,
    #[error("reference_block past validity window")]
    ExpiredRefBlock,
    #[error("mempool at capacity")]
    PoolFull,
    /// Per #5083, non-zero-value injected transactions are not yet
    /// supported. Reject at insert so the pool never holds one — the
    /// proposer cannot accidentally select it and the runtime won't
    /// charge a panicking program for an out-of-budget transfer.
    #[error("non-zero value injected txs are not yet supported (#5083)")]
    NonZeroValue,
}

impl MempoolInsertError {
    /// The tx is already known to a validator (either pooled or recently
    /// committed) — its promise will still fire, so RPC callers can keep
    /// watching for the reply.
    pub fn is_already_pooled(&self) -> bool {
        matches!(self, Self::AlreadyCommitted | Self::Duplicate)
    }
}

/// Surface a mempool insert outcome as a typed acceptance: `AlreadyPooled`
/// for `AlreadyCommitted` / `Duplicate` (promise still fires), `Reject` for
/// fatal cases.
pub fn classify_insert_outcome(
    outcome: Result<(), MempoolInsertError>,
) -> ethexe_common::injected::InjectedTransactionAcceptance {
    use ethexe_common::injected::InjectedTransactionAcceptance;
    match outcome {
        Ok(()) => InjectedTransactionAcceptance::Accept,
        Err(err) if err.is_already_pooled() => InjectedTransactionAcceptance::AlreadyPooled {
            reason: err.to_string(),
        },
        Err(err) => InjectedTransactionAcceptance::Reject {
            reason: err.to_string(),
        },
    }
}

/// Producer-side source of injected transactions. Fetch is non-destructive;
/// `forget` runs after MB finalization and dedups within `VALIDITY_WINDOW`.
#[async_trait]
pub trait Mempool: Send + Sync + 'static {
    /// Returns `Err` for the reasons in [`MempoolInsertError`]; callers map
    /// the result to an `InjectedTransactionAcceptance`.
    fn insert(&self, tx: SignedInjectedTransaction) -> Result<(), MempoolInsertError>;

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

/// Always-empty mempool, useful to bring up the service on an idle node.
#[derive(Clone, Default)]
pub struct EmptyMempool;

#[async_trait]
impl Mempool for EmptyMempool {
    fn insert(&self, _tx: SignedInjectedTransaction) -> Result<(), MempoolInsertError> {
        Ok(())
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
    fn insert(&self, tx: SignedInjectedTransaction) -> Result<(), MempoolInsertError> {
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
            return Err(MempoolInsertError::NonZeroValue);
        }

        let inner = self.inner.lock().expect("poisoned mempool");

        if inner.seen.contains_key(&tx_hash) {
            info!(%tx_hash, "mempool: rejecting tx — hash already committed within validity window");
            return Err(MempoolInsertError::AlreadyCommitted);
        }

        if inner.pool.contains_key(&tx_hash) {
            info!(%tx_hash, pool_len = inner.pool.len(), "mempool: skip — duplicate insert");
            return Err(MempoolInsertError::Duplicate);
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
            return Err(MempoolInsertError::ExpiredRefBlock);
        }

        if inner.pool.len() >= self.capacity {
            info!(%tx_hash, capacity = self.capacity, "mempool: rejecting tx — pool at capacity");
            return Err(MempoolInsertError::PoolFull);
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
            return Err(MempoolInsertError::AlreadyCommitted);
        }
        if inner.pool.contains_key(&tx_hash) {
            return Err(MempoolInsertError::Duplicate);
        }
        if inner.pool.len() >= self.capacity {
            return Err(MempoolInsertError::PoolFull);
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
        Ok(())
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

    /// Pins the link between [`MempoolInsertError`] variants and the
    /// `AlreadyPooled` / `Reject` classification consumed by RPC fan-out.
    /// Adding a variant without updating [`MempoolInsertError::is_already_pooled`]
    /// will be caught here.
    #[test]
    fn classify_insert_outcome_maps_each_variant() {
        assert!(matches!(
            classify_insert_outcome(Ok(())),
            InjectedTransactionAcceptance::Accept
        ));
        for err in [
            MempoolInsertError::AlreadyCommitted,
            MempoolInsertError::Duplicate,
        ] {
            assert!(
                matches!(
                    classify_insert_outcome(Err(err)),
                    InjectedTransactionAcceptance::AlreadyPooled { .. }
                ),
                "already-pooled variant must classify as AlreadyPooled",
            );
        }
        for err in [
            MempoolInsertError::ExpiredRefBlock,
            MempoolInsertError::PoolFull,
            MempoolInsertError::NonZeroValue,
        ] {
            assert!(
                matches!(
                    classify_insert_outcome(Err(err)),
                    InjectedTransactionAcceptance::Reject { .. }
                ),
                "fatal variant must classify as Reject",
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
        pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 0))
            .unwrap();

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

        assert!(matches!(
            pool.insert(value_tx),
            Err(MempoolInsertError::NonZeroValue),
        ));
        assert_eq!(pool.len(), 1, "non-zero-value tx must not enter the pool");
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
        pool.insert(tx).unwrap();
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

        pool.insert(tx.clone()).unwrap();
        assert_eq!(pool.len(), 1);

        // The pool fetches when ref_block is on the canonical chain
        // of the head we hand it.
        let head = chain[2];
        let fetched = futures::executor::block_on(pool.fetch(head));
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].data().to_hash(), tx_hash);
    }

    #[test]
    fn duplicate_insert_is_no_op() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::new(db);

        let pk = PrivateKey::random();
        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 7);
        pool.insert(tx.clone()).unwrap();
        assert_eq!(pool.len(), 1);
        assert!(matches!(
            pool.insert(tx),
            Err(MempoolInsertError::Duplicate)
        ));
        assert_eq!(pool.len(), 1, "duplicate by hash should be a no-op");
    }

    #[test]
    fn capacity_limit_blocks_further_inserts() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::with_capacity(db, 2);

        let pk = PrivateKey::random();
        pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 0))
            .unwrap();
        pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 1))
            .unwrap();
        assert!(matches!(
            pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 2)),
            Err(MempoolInsertError::PoolFull),
        ));
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
            pool.insert(signed_tx(&pk, ActorId::zero(), bogus_ref_block, salt))
                .unwrap();
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
        pool.insert(tx).unwrap();
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
        pool.insert(tx.clone()).unwrap();
        assert_eq!(pool.len(), 1);

        futures::executor::block_on(pool.forget(std::slice::from_ref(&tx)));
        assert_eq!(pool.len(), 0);

        // Re-inserting the same tx must be rejected (seen-hash hit).
        assert!(matches!(
            pool.insert(tx),
            Err(MempoolInsertError::AlreadyCommitted),
        ));
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
        pool.insert(tx_alt).unwrap();
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
        pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, 0))
            .unwrap();

        // Waiter should now wake up promptly.
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("wait_for_new_tx must unblock after insert")
            .expect("waiter task panicked");
    }

    #[tokio::test(start_paused = true)]
    async fn wait_for_new_tx_does_not_wake_on_rejected_insert() {
        // A duplicate / capped insert should not wake a waiter — Notify
        // is signalled only on a successful insert.
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = std::sync::Arc::new(InjectedTxMempool::new(db));
        let pk = PrivateKey::random();
        let tx = signed_tx(&pk, ActorId::zero(), chain[1].hash, 0);

        // Seed one accepted insert and consume the resulting permit so
        // the next `.notified()` re-blocks until the next signal.
        pool.insert(tx.clone()).unwrap();
        pool.wait_for_new_tx().await;

        let waiter = {
            let pool = pool.clone();
            tokio::spawn(async move {
                pool.wait_for_new_tx().await;
            })
        };

        tokio::time::sleep(Duration::from_millis(10)).await;

        // Same tx hash — rejected as duplicate, no signal.
        assert!(matches!(
            pool.insert(tx),
            Err(MempoolInsertError::Duplicate)
        ));

        // Waiter must still be pending.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !waiter.is_finished(),
            "waiter must stay blocked when insert was rejected"
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
                        if pool.insert(tx.clone()).is_ok() {
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
                pool.insert(signed_tx(&pk, ActorId::zero(), anchor, i as u8)).unwrap();
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
            pool.insert(tx.clone()).unwrap();
            prop_assert_eq!(pool.len(), 1);
            futures::executor::block_on(pool.forget(std::slice::from_ref(&tx)));
            prop_assert_eq!(pool.len(), 0);
            // Re-insert: rejected because the hash sits in the
            // seen-set and `reference_block` hasn't aged out.
            prop_assert!(matches!(
                pool.insert(tx),
                Err(MempoolInsertError::AlreadyCommitted)
            ));
            prop_assert_eq!(pool.len(), 0);
        }
    }
}
