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
//! Very simple for MVP: a FIFO-ordered map keyed by SCALE-encoded tx
//! hash. No prioritization, no eviction other than size cap, no gas
//! accounting beyond fitting-into-budget. Good enough to wire the
//! plumbing — it will grow when the execution side of ethexe starts
//! feeding it back validation signals.

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use ethexe_common::injected::SignedInjectedTransaction;

use crate::Mempool;

/// Default cap on the number of pending TXs the in-memory pool holds.
pub const DEFAULT_POOL_CAPACITY: usize = 10_000;

#[derive(Debug)]
pub struct InjectedTxMempool {
    inner: Mutex<VecDeque<SignedInjectedTransaction>>,
    capacity: usize,
}

impl Default for InjectedTxMempool {
    fn default() -> Self {
        Self::new(DEFAULT_POOL_CAPACITY)
    }
}

impl InjectedTxMempool {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(VecDeque::new()),
            capacity,
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("poisoned mempool").len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().expect("poisoned mempool").is_empty()
    }
}

#[async_trait]
impl Mempool for InjectedTxMempool {
    /// Accept a transaction into the pool. Currently no de-duplication —
    /// callers upstream (RPC / network) are expected to filter.
    fn insert(&self, tx: SignedInjectedTransaction) {
        let mut pool = self.inner.lock().expect("poisoned mempool");
        if pool.len() >= self.capacity {
            pool.pop_front();
        }
        pool.push_back(tx);
    }

    async fn fetch(&self, _gas_budget: u64) -> Vec<SignedInjectedTransaction> {
        // Drain the pool entirely — gas accounting not yet wired. Once
        // it is, we'll walk the queue and stop when the cumulative gas
        // exceeds the budget.
        let mut pool = self.inner.lock().expect("poisoned mempool");
        std::mem::take(&mut *pool).into_iter().collect()
    }

    async fn forget(&self, _committed: &[SignedInjectedTransaction]) {
        // No-op: `fetch` already drained everything. When we stop
        // draining fully, `forget` will remove the specific TXs.
    }
}
