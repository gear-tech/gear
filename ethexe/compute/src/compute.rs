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
//! `compute_mb` walks the parent chain via [`CompactMB::parent`], runs any
//! uncomputed ancestors oldest-first, then the target. DB layout:
//! `mb_compact_block` (persisted by the service at finalize), `transactions`
//! (CAS payload), `mb_meta` (`computed` flips here), and the per-MB program
//! states / outcome / schedule rows on success.

use crate::{ComputeError, ComputeEvent, ProcessorExt, Result, service::SubService};
use ethexe_common::{
    db::{CodesStorageRW, ConfigStorageRO, MbStorageRO, MbStorageRW, OnChainStorageRO},
    events::BlockRequestEvent,
    injected::Promise,
    malachite::{Transaction, Transactions},
};
use ethexe_db::Database;
use ethexe_processor::ExecutableData;
use ethexe_runtime_common::FinalizedBlockTransitions;
use futures::{FutureExt, Stream, StreamExt, future::BoxFuture};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;

// +_+_+ use promise streaming mode here PromisePolicy
/// MB-execution request; payload is read from the DB by hash.
#[derive(Debug)]
pub(crate) struct MbComputeRequest {
    pub mb_hash: H256,
}

#[derive(Debug, Clone, Copy)]
struct MbComputeOk {
    mb_hash: H256,
    height: u64,
}

type ComputationFuture = BoxFuture<'static, Result<MbComputeOk>>;

/// Streams `ComputeEvent::Promise`s from the executor's per-MB channel; closes
/// when every sender (incl. thread-local ones) is dropped at compute end.
struct MbPromisesStream {
    receiver: mpsc::UnboundedReceiver<Promise>,
    mb_hash: H256,
}

impl Stream for MbPromisesStream {
    type Item = ComputeEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mb_hash = self.mb_hash;
        Poll::Ready(
            futures::ready!(self.receiver.poll_recv(cx))
                .map(|promise| ComputeEvent::Promise(promise, mb_hash)),
        )
    }
}

pub struct ComputeSubService<P: ProcessorExt> {
    db: Database,
    processor: P,

    input: VecDeque<MbComputeRequest>,
    computation: Option<ComputationFuture>,
    /// Per-MB promise channel; polled before `computation` so promises stream out live.
    promises_stream: Option<MbPromisesStream>,
    /// Held until `promises_stream` drains so `MbComputed` lands after the last promise.
    pending_event: Option<Result<ComputeEvent>>,
}

impl<P: ProcessorExt> ComputeSubService<P> {
    pub fn new(db: Database, processor: P) -> Self {
        Self {
            db,
            processor,
            input: VecDeque::new(),
            computation: None,
            promises_stream: None,
            pending_event: None,
        }
    }

    pub fn receive_mb(&mut self, mb_hash: H256) {
        self.input.push_back(MbComputeRequest { mb_hash });
    }

    async fn compute(
        db: Database,
        mut processor: P,
        req: MbComputeRequest,
        promise_out_tx: mpsc::UnboundedSender<Promise>,
    ) -> Result<MbComputeOk> {
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
            // Predecessor MBs ran on a previous chain head; we
            // execute them only to bring the local DB up to date,
            // not to publish their replies (other validators have
            // already gossiped those promises). Pass `None` for the
            // promise channel so we don't double-emit.
            for (height, hash, txs) in predecessors {
                Self::compute_one(&db, &mut processor, height, hash, txs, None).await?;
            }
        }

        let target_txs = db
            .transactions(target_compact.transactions_hash)
            .ok_or(ComputeError::MbBlockNotFound(target_hash))?;
        Self::compute_one(
            &db,
            &mut processor,
            target_height,
            target_hash,
            target_txs,
            Some(promise_out_tx),
        )
        .await?;

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
        promise_out_tx: Option<mpsc::UnboundedSender<Promise>>,
    ) -> Result<()> {
        let parent_mb_hash = db
            .mb_compact_block(mb_hash)
            .and_then(|c| (!c.parent.is_zero()).then_some(c.parent));

        let program_states = parent_mb_hash
            .and_then(|h| db.mb_program_states(h))
            .unwrap_or_default();
        let schedule = parent_mb_hash
            .and_then(|h| db.mb_schedule(h))
            .unwrap_or_default();
        let initial_advanced_block = parent_mb_hash
            .map(|h| db.mb_meta(h).last_advanced_eb)
            .unwrap_or_default();

        let _ = mb_height;
        let prepared =
            build_executable_data(db, block, program_states, schedule, initial_advanced_block)?;

        log::debug!(
            "mb-compute: executing MB height {mb_height} hash {mb_hash} \
             (parent {parent_mb_hash:?}, eth height {}, eth ts {}, events: {}, injected: {})",
            prepared.height,
            prepared.timestamp,
            prepared.events.len(),
            prepared.injected_transactions.len(),
        );

        let processing_result = processor.process_programs(prepared, promise_out_tx).await?;

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

/// Walk the MB's `Transactions` list and prepare processor input.
///
/// Synthetic block height/timestamp come from `last_advanced_eb` (the latest
/// EB pinned by this MB or any ancestor); if none, fall back to the router's
/// genesis block from [`ConfigStorageRO::config`].
fn build_executable_data(
    db: &Database,
    block: Transactions,
    program_states: ethexe_common::ProgramStates,
    schedule: ethexe_common::Schedule,
    initial_advanced_block: H256,
) -> Result<ExecutableData> {
    let mut events: Vec<BlockRequestEvent> = Vec::new();
    let mut injected_transactions = Vec::new();
    let mut gas_allowance: Option<u64> = None;
    let mut current_anchor = initial_advanced_block;

    for tx in block.0 {
        match tx {
            Transaction::AdvanceTillEthereumBlock { block_hash } => {
                let chain = collect_advance_chain(db, block_hash, current_anchor)?;
                for hash in chain {
                    let block_events = db.block_events(hash).unwrap_or_default();
                    for event in block_events.into_iter().filter_map(|e| e.to_request()) {
                        events.push(event);
                    }
                }
                current_anchor = block_hash;
            }
            Transaction::Injected(signed) => {
                let verified = signed.into_verified();
                injected_transactions.push(verified);
            }
            Transaction::ProgressTasks { limits: _ } => {}
            Transaction::ProcessQueues { limits } => {
                gas_allowance = Some(limits.gas_allowance);
            }
        }
    }

    let anchor_eth_block = if current_anchor.is_zero() {
        db.config().genesis_block_hash
    } else {
        current_anchor
    };

    let (height, timestamp) = db
        .block_header(anchor_eth_block)
        .map(|h| (h.height, h.timestamp))
        .unwrap_or((0, 0));

    Ok(ExecutableData {
        height,
        timestamp,
        program_states,
        schedule,
        injected_transactions,
        gas_allowance,
        events,
    })
}

/// EBs in `(last_advanced, target]`, oldest-first; capped at 1024.
fn collect_advance_chain(db: &Database, target: H256, last_advanced: H256) -> Result<Vec<H256>> {
    const MAX_ADVANCE_STEPS: usize = 1024;

    if target == last_advanced {
        return Ok(Vec::new());
    }

    let mut chain = Vec::new();
    let mut current = target;
    while current != last_advanced && current != H256::zero() {
        if chain.len() >= MAX_ADVANCE_STEPS {
            return Err(ComputeError::AdvanceWalkTooDeep {
                target,
                last_advanced,
            });
        }
        let Some(header) = db.block_header(current) else {
            if chain.is_empty() {
                return Err(ComputeError::AdvanceMissingHeader { hash: current });
            }
            break;
        };
        chain.push(current);
        current = header.parent_hash;
    }

    chain.reverse();
    Ok(chain)
}

/// Uncomputed ancestors of `target_hash`, oldest-first; stops at genesis or
/// the first computed parent. Errors on missing parent (DB invariant).
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

impl<P: ProcessorExt> SubService for ComputeSubService<P> {
    type Output = ComputeEvent;

    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Output>> {
        // (1) Pick up the next request whenever no work is in flight.
        if self.computation.is_none()
            && self.promises_stream.is_none()
            && self.pending_event.is_none()
            && let Some(req) = self.input.pop_front()
        {
            let mb_hash = req.mb_hash;
            let (sender, receiver) = mpsc::unbounded_channel();
            self.promises_stream = Some(MbPromisesStream { receiver, mb_hash });
            self.computation =
                Some(Self::compute(self.db.clone(), self.processor.clone(), req, sender).boxed());
        }

        // (2) Forward streaming promises before anything else so the
        // service handler sees them as the runtime emits them.
        if let Some(ref mut stream) = self.promises_stream
            && let Poll::Ready(maybe_event) = stream.poll_next_unpin(cx)
        {
            match maybe_event {
                Some(event) => return Poll::Ready(Ok(event)),
                None => {
                    // Channel is fully drained — the executor has
                    // dropped every sender clone, which means
                    // `compute_one` is past the `process_transitions`
                    // await (and thus `computation` is at most a
                    // book-keeping step away from completing).
                    self.promises_stream = None;
                }
            }
        }

        // (3) An MbComputed result waiting for the stream to close
        // gets released next.
        if let Some(event) = self.pending_event.take() {
            return Poll::Ready(event);
        }

        // (4) Drive the computation future. Hold the resulting
        // `MbComputed` back if the promise stream still has buffered
        // sends — preserves "all promises before MbComputed" ordering.
        if let Some(ref mut computation) = self.computation
            && let Poll::Ready(result) = computation.poll_unpin(cx)
        {
            self.computation = None;
            let event = result.map(|ok| ComputeEvent::MbComputed {
                mb_hash: ok.mb_hash,
                height: ok.height,
            });
            if self.promises_stream.is_some() {
                self.pending_event = Some(event);
                return Poll::Pending;
            }
            return Poll::Ready(event);
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::MockProcessor;
    use ethexe_common::{
        BlockHeader,
        db::{CompactMB, OnChainStorageRW},
        malachite::{ProcessQueuesLimits, ProgressTasksLimits, Transaction},
    };

    fn dummy_txs(db: &Database, tag: u8) -> Transactions {
        // Tag-derived AdvanceTillEthereumBlock makes each block's
        // transaction list (and thus its CAS hash) unique across heights.
        // The referenced EB also needs a header in the DB so the
        // compute-side advance walk picks it up.
        let eth_block_hash = H256::from_low_u64_be(0xEB00 + tag as u64);
        db.set_block_header(
            eth_block_hash,
            BlockHeader {
                height: tag as u32,
                timestamp: tag as u64,
                parent_hash: H256::zero(),
            },
        );
        db.set_block_events(eth_block_hash, &[]);
        Transactions::new(vec![
            Transaction::AdvanceTillEthereumBlock {
                block_hash: eth_block_hash,
            },
            Transaction::ProgressTasks {
                limits: ProgressTasksLimits::default(),
            },
            Transaction::ProcessQueues {
                limits: ProcessQueuesLimits::default(),
            },
        ])
    }

    /// Mimics malachite `save_block`: CAS write + `CompactMB`.
    fn seed_mb(db: &Database, mb_hash: H256, parent: H256, height: u64, txs: Transactions) {
        let transactions_hash = db.set_transactions(txs);
        db.set_mb_compact_block(
            mb_hash,
            CompactMB {
                parent,
                height,
                transactions_hash,
            },
        );
    }

    /// Tail-only queue still computes all uncomputed predecessors.
    #[tokio::test]
    #[ntest::timeout(5000)]
    async fn walks_uncomputed_predecessors() {
        let db = Database::memory();
        let processor = MockProcessor::default();
        let mut sub = ComputeSubService::new(db.clone(), processor);

        // 5-block chain; mb_hash = 0x1000 + i.
        const N: u64 = 5;
        let mut hashes = Vec::with_capacity(N as usize);
        let mut parent = H256::zero();
        for i in 1..=N {
            let mb_hash = H256::from_low_u64_be(0x1000 + i);
            seed_mb(&db, mb_hash, parent, i, dummy_txs(&db, i as u8));
            hashes.push((i, mb_hash));
            parent = mb_hash;
        }

        // Sanity: nothing computed yet.
        for (_, hash) in &hashes {
            assert!(!db.mb_meta(*hash).computed);
        }

        // Tail-only queue forces walking back through 4 ancestors.
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

        // All ancestors must end up computed.
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
        let mut sub = ComputeSubService::new(db.clone(), processor);

        let mb_hash = H256::from_low_u64_be(0xCAFE);
        seed_mb(&db, mb_hash, H256::zero(), 1, dummy_txs(&db, 0));
        db.mutate_mb_meta(mb_hash, |meta| {
            meta.computed = true; // pretend a previous run finished it
        });

        sub.receive_mb(mb_hash);

        let event = sub.next().await.unwrap();
        match event {
            ComputeEvent::MbComputed {
                mb_hash: out,
                height,
            } => {
                assert_eq!(out, mb_hash);
                assert_eq!(height, 1);
            }
            other => panic!("expected MbComputed, got {other:?}"),
        }
    }
}
