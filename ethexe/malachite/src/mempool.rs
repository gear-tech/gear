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

//! In-memory pool of injected transactions that the Malachite
//! sequencer draws from when this node is the block producer.
//!
//! Lifecycle rules (see also `ethexe-consensus/src/tx_validation.rs`):
//!
//! - Every tx carries `reference_block: H256`. The tx is valid as
//!   long as `ref_block.height + VALIDITY_WINDOW > head.height`.
//! - On insert we drop any tx whose `ref_block` is already outside
//!   the validity window relative to the latest observed head, or
//!   whose `ref_block` is not yet in the database.
//! - On fetch we return only txs whose `ref_block` is a canonical
//!   ancestor of the given `head`. Non-ancestors are kept — a reorg
//!   can make them eligible again.
//! - On forget (finalized MB) we remove the tx from the pool and
//!   remember its hash in a seen-hash table. Subsequent inserts of
//!   the same tx are rejected. Seen-hashes age out by the same
//!   VALIDITY_WINDOW rule as pool entries.
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
    db::{GlobalsStorageRO, OnChainStorageRO},
    injected::{InjectedTransaction, SignedInjectedTransaction, VALIDITY_WINDOW},
};
use ethexe_db::Database;
use gprimitives::H256;
use tokio::sync::Notify;
use tracing::{debug, trace};

use crate::Mempool;

/// Default cap on the number of pending TXs the in-memory pool holds.
/// We start rejecting new inserts once this is reached — better than
/// silently dropping old entries that might still be the only copy
/// the network has.
pub const DEFAULT_POOL_CAPACITY: usize = 10_000;

/// Internal pool state — protected by a single [`Mutex`] because all
/// operations are quick and the pool sees low contention
/// (producer-only writes from the RPC/network ingress tasks).
#[derive(Debug, Default)]
struct Inner {
    /// Pending txs keyed by their canonical hash.
    pool: HashMap<HashOf<InjectedTransaction>, SignedInjectedTransaction>,
    /// Hashes of txs that have already been included in a finalized
    /// MB, together with the `reference_block` they carried. We keep
    /// them around so a re-gossipped duplicate can't slip back into
    /// the pool; entries are evicted when their `reference_block`
    /// ages out (same rule as pool txs).
    seen: HashMap<HashOf<InjectedTransaction>, H256>,
    /// Height of the latest chain head observed via
    /// [`Mempool::set_chain_head`]. Any tx whose `reference_block`
    /// height falls ≤ `latest_head_height - VALIDITY_WINDOW` is
    /// considered expired.
    latest_head_height: Option<u32>,
}

#[derive(Debug)]
pub struct InjectedTxMempool {
    inner: Mutex<Inner>,
    db: Database,
    capacity: usize,
    /// Signal raised on every successful tx insert. The producer
    /// awaits on this from `Mempool::wait_for_new_tx` so it can wake
    /// out of an idle wait the moment a fresh tx lands.
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

    /// True when `ref_block` has aged past the validity window
    /// relative to the given `head_height`.
    fn is_expired(head_height: u32, ref_block_height: u32) -> bool {
        // Matches tx_validation.rs: tx is valid while
        //   ref_block_height <= head && ref_block_height + WINDOW > head.
        // So it's expired when the second part fails. `saturating_add`
        // guards against u32 overflow if ref_block is close to
        // u32::MAX (academic but cheap).
        ref_block_height.saturating_add(VALIDITY_WINDOW as u32) <= head_height
    }

    /// The oldest block the local DB is guaranteed to have a header
    /// for. Walks stop here; going past it would read a parent that
    /// isn't in our DB.
    fn start_block_hash(&self) -> H256 {
        self.db.globals().start_block_hash
    }

    /// Build the set of ancestor hashes of `head` reachable within
    /// `VALIDITY_WINDOW` parent steps. Walk stops at `start_block`
    /// (or earlier if a header happens to be missing). Used to
    /// answer "is this tx's ref_block on the current branch?".
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
    fn purge_expired(inner: &mut Inner, head_height: u32, db: &Database) {
        inner.pool.retain(|tx_hash, tx| {
            let ref_block = tx.data().reference_block;
            match db.block_header(ref_block).map(|h| h.height) {
                Some(h) if !Self::is_expired(head_height, h) => true,
                _ => {
                    trace!(%tx_hash, %ref_block, "dropping expired tx from pool");
                    false
                }
            }
        });

        inner.seen.retain(|tx_hash, ref_block| {
            match db.block_header(*ref_block).map(|h| h.height) {
                Some(h) if !Self::is_expired(head_height, h) => true,
                _ => {
                    trace!(%tx_hash, ref_block = %ref_block, "dropping expired seen-hash");
                    false
                }
            }
        });
    }
}

#[async_trait]
impl Mempool for InjectedTxMempool {
    fn insert(&self, tx: SignedInjectedTransaction) {
        let tx_data = tx.data();
        let tx_hash = tx_data.to_hash();
        let ref_block = tx_data.reference_block;

        let mut inner = self.inner.lock().expect("poisoned mempool");

        if inner.seen.contains_key(&tx_hash) {
            debug!(%tx_hash, "rejecting tx: hash already committed within validity window");
            return;
        }

        if inner.pool.contains_key(&tx_hash) {
            return;
        }

        // ref_block must resolve to a known header. If it doesn't:
        // - the tx references a block we haven't synced yet (let the
        //   sender re-gossip after our DB catches up),
        // - or it's older than our start_block (we can't verify
        //   ancestry locally, reject).
        // Either way: don't hold onto something we can't reason about.
        let Some(ref_height) = self.ref_block_height(ref_block) else {
            debug!(
                %tx_hash, %ref_block,
                "rejecting tx: reference_block not in DB"
            );
            return;
        };
        if let Some(head_height) = inner.latest_head_height {
            if Self::is_expired(head_height, ref_height) {
                debug!(
                    %tx_hash, %ref_block, ref_height, head_height,
                    "rejecting tx: reference_block past VALIDITY_WINDOW"
                );
                return;
            }
        }

        if inner.pool.len() >= self.capacity {
            debug!(%tx_hash, capacity = self.capacity, "rejecting tx: pool at capacity");
            return;
        }

        inner.pool.insert(tx_hash, tx);
        // Drop the lock before signaling so a waiter resumed
        // immediately doesn't have to bounce on the mutex.
        drop(inner);
        self.new_tx_notify.notify_waiters();
    }

    fn set_chain_head(&self, head: SimpleBlockData) {
        let mut inner = self.inner.lock().expect("poisoned mempool");
        let h = head.header.height;
        if inner.latest_head_height == Some(h) {
            // Same height re-sent — nothing to GC beyond what we
            // already did on the previous call.
            return;
        }
        inner.latest_head_height = Some(h);
        Self::purge_expired(&mut inner, h, &self.db);
    }

    async fn fetch(
        &self,
        head: SimpleBlockData,
        _gas_budget: u64,
    ) -> Vec<SignedInjectedTransaction> {
        let ancestors = self.recent_ancestors(&head);

        let inner = self.inner.lock().expect("poisoned mempool");
        inner
            .pool
            .values()
            .filter(|tx| ancestors.contains(&tx.data().reference_block))
            .cloned()
            .collect()
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
        // `Notify::notified()` returns a future that's permitted to
        // miss notifications fired *before* it's registered, so the
        // caller must always re-check `fetch()` after we return —
        // matches the trait's "best-effort" contract.
        self.new_tx_notify.notified().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        BlockHeader, PrivateKey, SignedMessage, SimpleBlockData,
        db::{BlockMetaStorageRW, GlobalsStorageRW, OnChainStorageRW},
        injected::InjectedTransaction,
    };
    use gprimitives::ActorId;
    use std::time::Duration;

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
    fn insert_unknown_ref_block_is_rejected() {
        let db = Database::memory();
        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();
        // ref_block points at a hash that's not in the DB
        let tx = signed_tx(&pk, ActorId::zero(), H256::random(), 1);
        pool.insert(tx);
        assert_eq!(pool.len(), 0);
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
        let fetched = futures::executor::block_on(pool.fetch(head, 1_000_000));
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
        pool.insert(tx.clone());
        assert_eq!(pool.len(), 1);
        pool.insert(tx);
        assert_eq!(pool.len(), 1, "duplicate by hash should be a no-op");
    }

    #[test]
    fn capacity_limit_blocks_further_inserts() {
        let db = Database::memory();
        let chain = linear_chain(&db, 2);
        let pool = InjectedTxMempool::with_capacity(db, 2);

        let pk = PrivateKey::random();
        for i in 0..3 {
            pool.insert(signed_tx(&pk, ActorId::zero(), chain[1].hash, i));
        }
        assert_eq!(pool.len(), 2, "third insert must hit the capacity cap");
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
        pool.set_chain_head(chain[head_idx]);
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

        // Re-inserting the same tx must be rejected (seen-hash hit).
        pool.insert(tx);
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
        let _ = db.globals_mutate(|_| {});

        let pool = InjectedTxMempool::new(db);
        let pk = PrivateKey::random();

        // tx anchored to the ALT branch
        let tx_alt = signed_tx(&pk, ActorId::zero(), alt_hash, 1);
        pool.insert(tx_alt);
        assert_eq!(pool.len(), 1);

        // Fetching for canonical branch (chain[1]) — alt tx must NOT
        // surface.
        let fetched = futures::executor::block_on(pool.fetch(chain[1], 1_000_000));
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
    async fn wait_for_new_tx_does_not_wake_on_rejected_insert() {
        // A duplicate / capped / unknown-ref insert should not wake
        // a waiter — Notify is signalled only on a successful insert.
        let db = Database::memory();
        let pool = std::sync::Arc::new(InjectedTxMempool::new(db));

        let waiter = {
            let pool = pool.clone();
            tokio::spawn(async move {
                pool.wait_for_new_tx().await;
            })
        };

        tokio::time::sleep(Duration::from_millis(10)).await;

        // ref_block isn't in the DB → rejected.
        let pk = PrivateKey::random();
        pool.insert(signed_tx(&pk, ActorId::zero(), H256::random(), 0));

        // Waiter must still be pending.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !waiter.is_finished(),
            "waiter must stay blocked when insert was rejected"
        );
        waiter.abort();
    }
}
