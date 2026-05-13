// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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
//! `compute_mb` walks the parent chain via [`CompactMb::parent`], runs any
//! uncomputed ancestors oldest-first, then the target. DB layout:
//! `mb_compact_block` (persisted by the service at finalize), `transactions`
//! (CAS payload), `mb_meta` (`computed` flips here), and the per-MB program
//! states / outcome / schedule rows on success.

use crate::{ComputeError, ComputeEvent, ProcessorExt, Result, service::SubService};
use ethexe_common::{
    PromiseEmissionMode, PromisePolicy,
    db::{CodesStorageRW, CompactMb, ConfigStorageRO, MbStorageRO, MbStorageRW, OnChainStorageRO},
    events::BlockRequestEvent,
    injected::Promise,
    malachite::{Transaction, Transactions},
};
use ethexe_db::Database;
use ethexe_processor::{BoundPromiseSink, ExecutableData};
use ethexe_runtime_common::FinalizedBlockTransitions;
use futures::{FutureExt, Stream, StreamExt, future::BoxFuture};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;

/// MB-execution request; payload is read from the DB by hash.
///
/// `promise_policy` decides whether the runtime should emit promises
/// while executing the target MB. Predecessor MBs walked back through
/// `parent` follow [`ComputeSubService::promise_emission_mode`]
/// instead — `AlwaysEmit` re-emits, `ConsensusDriven` stays silent.
#[derive(Debug)]
pub(crate) struct MbComputeRequest {
    pub mb_hash: H256,
    pub promise_policy: PromisePolicy,
}

type ComputationFuture = future_timing::Timed<BoxFuture<'static, Result<H256>>>;

/// Metrics for the [`ComputeSubService`].
#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_compute_compute")]
struct Metrics {
    /// The latency of MB execution in seconds represented as f64.
    mb_processing_latency: metrics::Histogram,
}

/// Streams `ComputeEvent::Promise`s from the executor's per-MB channel; closes
/// when every sender (incl. thread-local ones) is dropped at compute end.
///
/// The MB hash arrives on the channel pre-tagged by [`BoundPromiseSink`].
struct MbPromisesStream {
    receiver: mpsc::UnboundedReceiver<(H256, Promise)>,
}

impl Stream for MbPromisesStream {
    type Item = ComputeEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(
            futures::ready!(self.receiver.poll_recv(cx))
                .map(|(mb_hash, promise)| ComputeEvent::Promise(promise, mb_hash)),
        )
    }
}

pub struct ComputeSubService<P: ProcessorExt> {
    db: Database,
    processor: P,
    /// Decides whether predecessor MBs walked through `parent` also
    /// emit promises. `AlwaysEmit` lets RPC nodes replaying the chain
    /// publish replies for every MB; the default `ConsensusDriven`
    /// keeps predecessors silent (their promises were already gossiped
    /// by the producer at the time).
    promise_emission_mode: PromiseEmissionMode,
    metrics: Metrics,

    input: VecDeque<MbComputeRequest>,
    computation: Option<ComputationFuture>,
    /// Per-MB promise channel; polled before `computation` so promises stream out live.
    promises_stream: Option<MbPromisesStream>,
    /// Held until `promises_stream` drains so `MbComputed` lands after the last promise.
    pending_event: Option<Result<ComputeEvent>>,
}

impl<P: ProcessorExt> ComputeSubService<P> {
    pub fn new(db: Database, processor: P) -> Self {
        Self::with_promise_mode(db, processor, PromiseEmissionMode::default())
    }

    pub fn with_promise_mode(
        db: Database,
        processor: P,
        promise_emission_mode: PromiseEmissionMode,
    ) -> Self {
        Self {
            db,
            processor,
            promise_emission_mode,
            metrics: Metrics::default(),
            input: VecDeque::new(),
            computation: None,
            promises_stream: None,
            pending_event: None,
        }
    }

    pub fn receive_mb(&mut self, mb_hash: H256, promise_policy: PromisePolicy) {
        self.input.push_back(MbComputeRequest {
            mb_hash,
            promise_policy,
        });
    }

    async fn compute(
        db: Database,
        mut processor: P,
        req: MbComputeRequest,
        promise_emission_mode: PromiseEmissionMode,
        promise_tx: mpsc::UnboundedSender<(H256, Promise)>,
    ) -> Result<H256> {
        let MbComputeRequest {
            mb_hash: head_mb_hash,
            promise_policy,
        } = req;

        // Idempotent: if the target has already been computed (e.g.,
        // service queued it again after restart), there's nothing to
        // do — emit the completion event right away.
        if db.mb_meta(head_mb_hash).computed {
            return Ok(head_mb_hash);
        }

        let uncomputed_chain = collect_uncomputed_chain(&db, head_mb_hash)?;

        log::debug!("walking {} uncomputed MBs", uncomputed_chain.len());
        for (mb_hash, compact_mb) in uncomputed_chain {
            let predecessor_sink = match (promise_emission_mode, promise_policy) {
                (PromiseEmissionMode::AlwaysEmit, _)
                | (PromiseEmissionMode::ConsensusDriven, PromisePolicy::Enabled)
                    if mb_hash == head_mb_hash =>
                {
                    Some(BoundPromiseSink::new(promise_tx.clone(), mb_hash))
                }
                _ => None,
            };
            Self::compute_one(&db, &mut processor, mb_hash, compact_mb, predecessor_sink).await?;
        }

        Ok(head_mb_hash)
    }

    async fn compute_one(
        db: &Database,
        processor: &mut P,
        mb_hash: H256,
        compact_mb: CompactMb,
        promise_sink: Option<BoundPromiseSink>,
    ) -> Result<()> {
        log::debug!("compute one MB: hash {mb_hash} {compact_mb}");

        let executable = prepare_executable_for_mb(db, mb_hash, compact_mb)?;
        let processing_result = processor.process_programs(executable, promise_sink).await?;

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

/// Builds executable data for a single MB, parent MB must be computed.
pub fn prepare_executable_for_mb(
    db: &Database,
    mb_hash: H256,
    compact_mb: CompactMb,
) -> Result<ExecutableData> {
    let CompactMb {
        parent,
        transactions_hash,
        ..
    } = compact_mb;

    let mb_payload = db
        .transactions(transactions_hash)
        .ok_or(ComputeError::MbPayloadNotFound {
            mb_hash,
            payload_hash: transactions_hash,
        })?;

    let (program_states, schedule, initial_advanced_block) = if parent.is_zero() {
        // Genesis MB has no parent, so start with empty states and the router's genesis block as the anchor.
        (Default::default(), Default::default(), H256::zero())
    } else {
        let states = db
            .mb_program_states(parent)
            .ok_or(ComputeError::ParentMbStatesMissing(parent))?;
        let schedule = db
            .mb_schedule(parent)
            .ok_or(ComputeError::ParentMbScheduleMissing(parent))?;
        (states, schedule, db.mb_meta(parent).last_advanced_eb)
    };

    build_executable_data(
        db,
        mb_payload,
        program_states,
        schedule,
        initial_advanced_block,
    )
}

/// Walk the MB's `Transactions` list and prepare processor input.
///
/// Synthetic block height/timestamp come from `last_advanced_eb` (the latest
/// EB pinned by this MB or any ancestor); if none, fall back to the router's
/// genesis block from [`ConfigStorageRO::config`].
fn build_executable_data(
    db: &Database,
    transactions: Transactions,
    program_states: ethexe_common::ProgramStates,
    schedule: ethexe_common::Schedule,
    initial_advanced_block: H256,
) -> Result<ExecutableData> {
    let mut events: Vec<BlockRequestEvent> = Vec::new();
    let mut injected_transactions = Vec::new();
    let mut gas_allowance: Option<u64> = None;
    let mut current_anchor = initial_advanced_block;

    for tx in transactions.0 {
        match tx {
            Transaction::AdvanceTillEthereumBlock { block_hash } => {
                let chain = collect_advance_chain(db, block_hash, current_anchor)?;
                for hash in chain {
                    let block_events = db
                        .block_events(hash)
                        .ok_or(ComputeError::AdvanceBlockEventsMissing(hash))?;
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
        .ok_or(ComputeError::AnchorBlockHeaderMissing(anchor_eth_block))?;

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

/// Collect a chain of uncomputed MBs, beginning from head `mb_hash`, oldest-first;
/// Stops at the first computed ancestor or genesis (inclusive).
/// Returns an error if any MB in the chain is missing from the DB.
fn collect_uncomputed_chain(
    db: &Database,
    head_mb_hash: H256,
) -> Result<VecDeque<(H256, CompactMb)>> {
    let mut chain = VecDeque::new();
    let mut mb_hash = head_mb_hash;
    while !mb_hash.is_zero() && !db.mb_meta(mb_hash).computed {
        let compact_mb = db
            .mb_compact_block(mb_hash)
            .ok_or(ComputeError::MbCompactNotFound(mb_hash))?;
        chain.push_front((mb_hash, compact_mb));
        mb_hash = compact_mb.parent;
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
            let (sender, receiver) = mpsc::unbounded_channel();
            self.promises_stream = Some(MbPromisesStream { receiver });
            self.computation = Some(future_timing::timed(
                Self::compute(
                    self.db.clone(),
                    self.processor.clone(),
                    req,
                    self.promise_emission_mode,
                    sender,
                )
                .boxed(),
            ));
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
                    // `compute_one` is past the `process_programs`
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
            && let Poll::Ready(timing_result) = computation.poll_unpin(cx)
        {
            let (timing, result) = timing_result.into_parts();
            self.metrics
                .mb_processing_latency
                .record((timing.busy() + timing.idle()).as_secs_f64());

            self.computation = None;
            let event = result.map(ComputeEvent::MbComputed);
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
        db::{CompactMb, OnChainStorageRW},
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

    /// Mimics malachite `save_block`: CAS write + `CompactMb`.
    fn seed_mb(db: &Database, mb_hash: H256, parent: H256, height: u64, txs: Transactions) {
        let transactions_hash = db.set_transactions(txs);
        db.set_mb_compact_block(
            mb_hash,
            CompactMb {
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
        gear_utils::init_default_logger();

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
        let (_tail_height, tail_hash) = *hashes.last().unwrap();
        sub.receive_mb(tail_hash, ::ethexe_common::PromisePolicy::Enabled);

        let event = sub.next().await.unwrap();
        match event {
            ComputeEvent::MbComputed(mb_hash) => assert_eq!(mb_hash, tail_hash),
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

        sub.receive_mb(mb_hash, ::ethexe_common::PromisePolicy::Enabled);

        let event = sub.next().await.unwrap();
        match event {
            ComputeEvent::MbComputed(out) => assert_eq!(out, mb_hash),
            other => panic!("expected MbComputed, got {other:?}"),
        }
    }
}
