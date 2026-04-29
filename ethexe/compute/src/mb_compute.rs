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

//! Per-MB execution sub-service.
//!
//! Once a Malachite sequencer block has been finalized, the outer
//! service feeds it into this sub-service via
//! [`ComputeService::compute_mb`](crate::ComputeService::compute_mb).
//! For every requested MB the sub-service first walks the parent
//! chain (via `mb_meta.parent_mb_hash`), collecting any ancestors
//! that the DB says are not yet computed — this catches uncomputed
//! MBs left behind by a crash between malachite-side persistence and
//! our finishing the execution. The collected predecessors then run
//! oldest-first, followed by the original target.
//!
//! Mirrors the announce pipeline in `compute.rs` (which walks
//! `announce.parent` and computes uncomputed predecessors first).
//!
//! # DB layout used here
//! - `mb_compact_block(hash) -> CompactBlock` — persisted by the
//!   service at `BlockFinalized` time so the walk can pull ancestor
//!   identity (parent + height + transactions_hash).
//! - `transactions(hash) -> Transactions` — CAS-stored payload,
//!   referenced by `CompactBlock::transactions_hash`.
//! - `mb_meta(hash) -> { computed, synced, last_advanced_block }` —
//!   we flip `computed = true` here once execution finishes.
//! - `mb_program_states / mb_outcome / mb_schedule(hash)` — written
//!   on successful execution.
//!
//! Hooking the MB results into Ethereum batch commitments is a
//! follow-up step.

use crate::{ComputeError, ComputeEvent, ProcessorExt, Result, service::SubService};
use ethexe_common::{
    BlockHeader, SimpleBlockData,
    db::{CodesStorageRW, MbStorageRO, MbStorageRW},
    mb::Transactions,
};
use ethexe_db::Database;
use ethexe_runtime_common::FinalizedBlockTransitions;
use futures::{FutureExt, future::BoxFuture};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    task::{Context, Poll},
};

/// Single MB-execution request queued up for the sub-service.
///
/// `mb_hash` is the consensus envelope hash (Blake2b over
/// `ethexe_malachite_core::Block`) under which the malachite service
/// has stored the matching [`crate::CompactBlock`] + transactions
/// blob. The compute layer reads both back from the DB on demand —
/// the request only carries the hash; the per-step gas budget lives
/// inside each `Transaction::ProcessQueues` payload.
#[derive(Debug)]
pub(crate) struct MbComputeRequest {
    pub mb_hash: H256,
}

/// Successful completion payload — the values a [`ComputeEvent::MbComputed`]
/// needs to carry upward.
#[derive(Debug, Clone, Copy)]
struct MbComputeOk {
    mb_hash: H256,
    height: u64,
}

type ComputationFuture = BoxFuture<'static, Result<MbComputeOk>>;

pub struct MbComputeSubService<P: ProcessorExt> {
    db: Database,
    processor: P,

    input: VecDeque<MbComputeRequest>,
    computation: Option<ComputationFuture>,
}

impl<P: ProcessorExt> MbComputeSubService<P> {
    pub fn new(db: Database, processor: P) -> Self {
        Self {
            db,
            processor,
            input: VecDeque::new(),
            computation: None,
        }
    }

    pub fn receive_mb(&mut self, mb_hash: H256) {
        self.input.push_back(MbComputeRequest { mb_hash });
    }

    async fn compute(db: Database, mut processor: P, req: MbComputeRequest) -> Result<MbComputeOk> {
        let target_hash = req.mb_hash;
        let target_compact = db
            .mb_compact_block(target_hash)
            .ok_or(ComputeError::MbBlockNotFound(target_hash))?;
        let target_height = target_compact.height;

        // Idempotent: if the target has already been computed (e.g.,
        // service queued it again after restart), there's nothing to
        // do — emit the completion event right away.
        if db.mb_meta(target_hash).computed {
            return Ok(MbComputeOk {
                mb_hash: target_hash,
                height: target_height,
            });
        }

        // Walk back from the target via `mb_compact_block.parent`,
        // collecting uncomputed predecessors. Linear heights mean
        // each step simply decrements by 1. We stop at:
        // - the genesis predecessor (parent is `H256::zero()`), or
        // - the first computed ancestor (already done).
        let predecessors = collect_uncomputed_predecessors(&db, target_hash, target_height)?;

        if !predecessors.is_empty() {
            log::info!(
                "mb-compute: walking {} uncomputed predecessor(s) before MB height {} hash {}",
                predecessors.len(),
                target_height,
                target_hash,
            );
        }

        for (height, hash, txs) in predecessors {
            Self::compute_one(&db, &mut processor, height, hash, txs).await?;
        }

        let target_txs = db
            .transactions(target_compact.transactions_hash)
            .ok_or(ComputeError::MbBlockNotFound(target_hash))?;
        Self::compute_one(&db, &mut processor, target_height, target_hash, target_txs).await?;

        Ok(MbComputeOk {
            mb_hash: target_hash,
            height: target_height,
        })
    }

    async fn compute_one(
        db: &Database,
        processor: &mut P,
        mb_height: u64,
        mb_hash: H256,
        block: Transactions,
    ) -> Result<()> {
        // Parent linkage lives in `mb_compact_block`, populated by the
        // malachite service before BlockProposal fires for `mb_hash`.
        let parent_mb_hash = db
            .mb_compact_block(mb_hash)
            .and_then(|c| (!c.parent.is_zero()).then_some(c.parent));

        let initial_program_states = parent_mb_hash
            .and_then(|h| db.mb_program_states(h))
            .unwrap_or_default();
        let initial_schedule = parent_mb_hash
            .and_then(|h| db.mb_schedule(h))
            .unwrap_or_default();
        // The processor walks the canonical Eth chain starting at
        // `last_advanced_block + 1` for each `AdvanceTillEthereumBlock`
        // tx, so it needs the parent MB's anchor as the seed value.
        // For genesis MB this is `H256::zero()`.
        let initial_advanced_block = parent_mb_hash
            .map(|h| db.mb_meta(h).last_advanced_block)
            .unwrap_or_default();

        // Synthetic block header per MVP convention agreed with the
        // user: height/timestamp both come from the MB number. The
        // `parent_hash` is the parent MB hash (or zero for the very
        // first MB) — this is purely traceability, no part of the
        // executor depends on its value.
        let synthetic_block = SimpleBlockData {
            hash: mb_hash,
            header: BlockHeader {
                height: mb_height as u32,
                timestamp: mb_height,
                parent_hash: parent_mb_hash.unwrap_or_default(),
            },
        };

        log::debug!(
            "mb-compute: executing MB height {} hash {} (parent {:?}, {} txs)",
            mb_height,
            mb_hash,
            parent_mb_hash,
            block.len(),
        );

        let processing_result = processor
            .process_transitions(
                initial_program_states,
                initial_schedule,
                synthetic_block,
                block.0,
                None,
                initial_advanced_block,
            )
            .await?;

        let FinalizedBlockTransitions {
            transitions,
            states,
            schedule,
            program_creations,
        } = processing_result;

        program_creations
            .into_iter()
            .for_each(|(program_id, code_id)| {
                db.set_program_code_id(program_id, code_id);
            });

        db.set_mb_outcome(mb_hash, transitions);
        db.set_mb_program_states(mb_hash, states);
        db.set_mb_schedule(mb_hash, schedule);
        db.mutate_mb_meta(mb_hash, |meta| {
            meta.computed = true;
        });

        Ok(())
    }
}

/// Walk the parent chain from `target_hash` collecting the
/// (height, hash, transactions) of every uncomputed ancestor —
/// oldest first.
///
/// Parent linkage is read from [`CompactBlock::parent`]. Stops at:
/// - genesis (parent is `H256::zero()`) — no further ancestors;
/// - the first ancestor with `mb_meta(hash).computed == true` —
///   everything older has already been processed in some earlier run.
///
/// Returns `Err(ComputeError::MbBlockNotFound)` if a parent referenced
/// from a child but missing from the local DB is encountered. That
/// only happens if the service didn't persist the block at
/// `BlockFinalized` time — i.e. an internal invariant violation.
fn collect_uncomputed_predecessors(
    db: &Database,
    target_hash: H256,
    target_height: u64,
) -> Result<VecDeque<(u64, H256, Transactions)>> {
    let mut chain = VecDeque::new();
    let mut current_parent = db
        .mb_compact_block(target_hash)
        .map(|c| c.parent)
        .unwrap_or(H256::zero());
    let mut current_height = target_height.saturating_sub(1);

    while !current_parent.is_zero() {
        if db.mb_meta(current_parent).computed {
            break;
        }
        let parent_compact = db
            .mb_compact_block(current_parent)
            .ok_or(ComputeError::MbBlockNotFound(current_parent))?;
        let parent_txs = db
            .transactions(parent_compact.transactions_hash)
            .ok_or(ComputeError::MbBlockNotFound(current_parent))?;
        chain.push_front((current_height, current_parent, parent_txs));
        current_parent = parent_compact.parent;
        current_height = current_height.saturating_sub(1);
    }

    Ok(chain)
}

impl<P: ProcessorExt> SubService for MbComputeSubService<P> {
    type Output = ComputeEvent;

    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Output>> {
        if self.computation.is_none()
            && let Some(req) = self.input.pop_front()
        {
            self.computation =
                Some(Self::compute(self.db.clone(), self.processor.clone(), req).boxed());
        }

        if let Some(ref mut computation) = self.computation
            && let Poll::Ready(result) = computation.poll_unpin(cx)
        {
            self.computation = None;
            return Poll::Ready(result.map(|ok| ComputeEvent::MbComputed {
                mb_hash: ok.mb_hash,
                height: ok.height,
            }));
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::MockProcessor;
    use ethexe_common::{
        db::CompactBlock,
        mb::{ProcessQueuesLimits, ProgressTasksLimits, Transaction},
    };

    fn dummy_txs(tag: u8) -> Transactions {
        // Tag-derived AdvanceTillEthereumBlock makes each block's
        // transaction list (and thus its CAS hash) unique across
        // heights.
        Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                eth_block_hash: H256::from_low_u64_be(0xEB00 + tag as u64),
            },
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            },
        ])
    }

    /// Service-side seeding helper. Stores `txs` in the CAS, writes a
    /// `CompactBlock` keyed by `mb_hash`, mirroring what the malachite
    /// `save_block` externalities do at finalize time.
    fn seed_mb(db: &Database, mb_hash: H256, parent: H256, height: u64, txs: Transactions) {
        let transactions_hash = db.set_transactions(txs);
        db.set_mb_compact_block(
            mb_hash,
            CompactBlock {
                parent,
                height,
                transactions_hash,
            },
        );
    }

    /// Crash-recovery walk: only the tail MB is queued, but every
    /// uncomputed predecessor in the parent chain ends up computed in
    /// height order.
    #[tokio::test]
    #[ntest::timeout(5000)]
    async fn walks_uncomputed_predecessors() {
        let db = Database::memory();
        let processor = MockProcessor::default();
        let mut sub = MbComputeSubService::new(db.clone(), processor);

        // Build a 5-block chain. Genesis's parent is `H256::zero()`.
        // Each subsequent block's parent is the previous block's
        // synthetic mb_hash (keyed `0x1000 + i`).
        const N: u64 = 5;
        let mut hashes = Vec::with_capacity(N as usize);
        let mut parent = H256::zero();
        for i in 1..=N {
            let mb_hash = H256::from_low_u64_be(0x1000 + i);
            seed_mb(&db, mb_hash, parent, i, dummy_txs(i as u8));
            hashes.push((i, mb_hash));
            parent = mb_hash;
        }

        // Sanity: nothing computed yet.
        for (_, hash) in &hashes {
            assert!(!db.mb_meta(*hash).computed);
        }

        // Queue ONLY the tail — the sub-service must walk back and
        // catch the previous four uncomputed MBs.
        let (tail_height, tail_hash) = *hashes.last().unwrap();
        sub.receive_mb(tail_hash);

        let event = sub.next().await.unwrap();
        match event {
            ComputeEvent::MbComputed { mb_hash, height } => {
                assert_eq!(mb_hash, tail_hash);
                assert_eq!(height, tail_height);
            }
            other => panic!("expected MbComputed, got {other:?}"),
        }

        // Every MB in the chain must now be marked computed. This
        // proves the walk visited every ancestor.
        for (i, hash) in &hashes {
            assert!(
                db.mb_meta(*hash).computed,
                "MB at height {i} should be computed"
            );
        }
    }

    /// Re-queueing an already-computed MB is a no-op (idempotent).
    #[tokio::test]
    #[ntest::timeout(5000)]
    async fn idempotent_for_computed_target() {
        let db = Database::memory();
        let processor = MockProcessor::default();
        let mut sub = MbComputeSubService::new(db.clone(), processor);

        let mb_hash = H256::from_low_u64_be(0xCAFE);
        seed_mb(&db, mb_hash, H256::zero(), 1, dummy_txs(0));
        db.mutate_mb_meta(mb_hash, |meta| {
            meta.computed = true; // pretend a previous run finished it
        });

        sub.receive_mb(mb_hash);

        let event = sub.next().await.unwrap();
        match event {
            ComputeEvent::MbComputed { mb_hash: out, height } => {
                assert_eq!(out, mb_hash);
                assert_eq!(height, 1);
            }
            other => panic!("expected MbComputed, got {other:?}"),
        }
    }
}
