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
