// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Per-MB execution sub-service.
//!
//! `compute_mb` walks the parent chain via [`CompactMb::parent`], runs any
//! uncomputed ancestors oldest-first, then the target. DB layout:
//! `mb_compact_block` (persisted by the service at finalize), `operations`
//! (CAS payload), `mb_meta` (`computed` flips here), and the per-MB program
//! states / outcome / schedule rows on success.

use crate::{ComputeError, ComputeEvent, ProcessorExt, Result, service::SubService};
use ethexe_common::{
    PromiseEmissionMode, PromisePolicy, SimpleBlockData,
    db::{
        BlockMetaStorageRO, CodesStorageRW, CompactMb, ConfigStorageRO, GlobalsStorageRO,
        MbStorageRO, MbStorageRW, OnChainStorageRO,
    },
    events::BlockRequestEvent,
    injected::Promise,
    malachite::{Operation, Operations},
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
    /// Requests whose prerequisite EB (the block this MB advances to) is not
    /// yet prepared in the DB. Held here instead of executing — and moved back
    /// into `input` by [`Self::receive_prepared_block`] once the prerequisite
    /// lands. This is the gate Malachite used to apply before emitting events;
    /// owning it here lets replayed/early MBs flow through as events while
    /// their execution waits for the code-validation pipeline to catch up.
    deferred: VecDeque<MbComputeRequest>,
    /// Head of the in-flight computation, kept so [`Self::receive_mb`] can
    /// skip duplicates that would otherwise re-emit `MbComputed`.
    in_flight_mb: Option<H256>,
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
            deferred: VecDeque::new(),
            in_flight_mb: None,
            computation: None,
            promises_stream: None,
            pending_event: None,
        }
    }

    pub fn receive_mb(&mut self, mb_hash: H256, promise_policy: PromisePolicy) {
        // Idempotent: skip if already computed, in flight, or queued
        // (`input` or `deferred`) — otherwise BlockProposal+BlockFinalized
        // for the same head emit `MbComputed` twice.
        if self.db.mb_meta(mb_hash).computed
            || self.in_flight_mb == Some(mb_hash)
            || self.input.iter().any(|r| r.mb_hash == mb_hash)
            || self.deferred.iter().any(|r| r.mb_hash == mb_hash)
        {
            return;
        }
        self.input.push_back(MbComputeRequest {
            mb_hash,
            promise_policy,
        });
    }

    /// An Ethereum block has been prepared: requeue every deferred request
    /// whose prerequisite EB is now satisfied. `PrepareSubService` prepares a
    /// whole ancestor chain but only reports the head, so we re-check all
    /// deferred requests rather than matching the reported hash.
    pub fn receive_prepared_block(&mut self, _eb_hash: H256) {
        let mut still_deferred = VecDeque::with_capacity(self.deferred.len());
        while let Some(req) = self.deferred.pop_front() {
            if self.db.mb_meta(req.mb_hash).computed {
                continue;
            }
            if self.prerequisite_ready(req.mb_hash) {
                self.input.push_back(req);
            } else {
                still_deferred.push_back(req);
            }
        }
        self.deferred = still_deferred;
    }

    /// Whether the EB this MB advances to is prepared (or there is none).
    /// Execution reads that EB's events, so it must wait until then.
    fn prerequisite_ready(&self, mb_hash: H256) -> bool {
        let eb = self.db.mb_meta(mb_hash).last_advanced_eb;
        eb.is_zero() || self.db.block_meta(eb).prepared
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
            // `AlwaysEmit` surfaces promises for every MB in the walked
            // chain — RPC nodes catching up need replies for predecessor
            // MBs too. `ConsensusDriven` emits only for the directly
            // requested head, and only when the caller opted in;
            // predecessors stay silent (already gossiped by the producer).
            let promise_sink = match (promise_emission_mode, promise_policy) {
                (PromiseEmissionMode::AlwaysEmit, _) => {
                    Some(BoundPromiseSink::new(promise_tx.clone(), mb_hash))
                }
                (PromiseEmissionMode::ConsensusDriven, PromisePolicy::Enabled)
                    if mb_hash == head_mb_hash =>
                {
                    Some(BoundPromiseSink::new(promise_tx.clone(), mb_hash))
                }
                _ => None,
            };
            Self::compute_one(&db, &mut processor, mb_hash, compact_mb, promise_sink).await?;
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
        operations_hash,
        ..
    } = compact_mb;

    let mb_payload = db
        .operations(operations_hash)
        .ok_or(ComputeError::MbPayloadNotFound {
            mb_hash,
            payload_hash: operations_hash,
        })?;

    // Read the parent MB's computed state from the DB. The genesis MB's parent
    // is the zero MB seeded by `initialize_empty_db` (carrying the genesis /
    // re-genesis state); it is a normal computed record like any other parent,
    // so no special-casing of the zero parent is needed here.
    let program_states = db
        .mb_program_states(parent)
        .ok_or(ComputeError::ParentMbStatesMissing(parent))?;
    let schedule = db
        .mb_schedule(parent)
        .ok_or(ComputeError::ParentMbScheduleMissing(parent))?;
    let advanced_block = db.mb_meta(parent).last_advanced_eb;

    build_executable_data(db, mb_payload, program_states, schedule, advanced_block)
}

/// Walk the MB's `Operations` list and prepare processor input.
///
/// Synthetic block height/timestamp come from `last_advanced_eb` (the latest
/// EB pinned by this MB or any ancestor); if none, fall back to the router's
/// genesis block from [`ConfigStorageRO::config`].
fn build_executable_data(
    db: &Database,
    operations: Operations,
    program_states: ethexe_common::ProgramStates,
    schedule: ethexe_common::Schedule,
    advanced_block: H256,
) -> Result<ExecutableData> {
    let mut events: Vec<BlockRequestEvent> = Vec::new();
    let mut injected_transactions = Vec::new();
    let mut gas_allowance: Option<u64> = None;

    let mut current_anchor = if advanced_block.is_zero() {
        None
    } else {
        Some(
            db.block_simple_data(advanced_block)
                .ok_or(ComputeError::AnchorBlockHeaderMissing(advanced_block))?,
        )
    };
    let mut mailbox_validity = ethexe_common::MAILBOX_VALIDITY_VERSION_2;
    let mut event_destinations_autoreply = false;

    for op in operations {
        match op {
            Operation::AdvanceTillEthereumBlock { block_hash } => {
                let block = db
                    .block_simple_data(block_hash)
                    .ok_or(ComputeError::AnchorBlockHeaderMissing(block_hash))?;
                let chain = collect_advance_chain(db, block, current_anchor)?;
                for hash in chain {
                    let block_events = db
                        .block_events(hash)
                        .ok_or(ComputeError::AdvanceBlockEventsMissing(hash))?;
                    for event in block_events.into_iter().filter_map(|e| e.to_request()) {
                        events.push(event);
                    }
                }
                current_anchor = Some(block);
            }
            Operation::Injected(signed) => {
                let verified = signed.into_verified();
                injected_transactions.push(verified);
            }
            Operation::ProgressTasks => {}
            Operation::ProcessQueues {
                gas_allowance: op_gas_allowance,
            } => {
                // Old block - change mailbox validity to the previous default one
                mailbox_validity = ethexe_common::MAILBOX_VALIDITY_VERSION_1;
                event_destinations_autoreply = false;

                gas_allowance = Some(op_gas_allowance);
            }
            Operation::ProcessQueuesV2 {
                gas_allowance: op_gas_allowance,
            } => {
                // Keep the new mailbox validity for this operation, as it is the default one
                event_destinations_autoreply = false;

                gas_allowance = Some(op_gas_allowance);
            }
            Operation::ProcessQueuesV3 {
                gas_allowance: op_gas_allowance,
            } => {
                // Keep V2 mailbox validity and enable V3 event-destination handling.
                event_destinations_autoreply = true;

                gas_allowance = Some(op_gas_allowance);
            }
        }
    }

    let (height, timestamp) = if let Some(current_anchor) = current_anchor {
        (
            current_anchor.header.height,
            current_anchor.header.timestamp,
        )
    } else {
        db.block_header(db.config().genesis_block_hash)
            .map(|h| (h.height, h.timestamp))
            .ok_or(ComputeError::Other(
                "genesis block missing from DB; invariant violation",
            ))?
    };

    Ok(ExecutableData {
        height,
        timestamp,
        program_states,
        schedule,
        injected_transactions,
        gas_allowance,
        events,
        mailbox_validity,
        event_destinations_autoreply,
    })
}

/// EBs in `(last_advanced, target]`, oldest-first;
fn collect_advance_chain(
    db: &Database,
    target: SimpleBlockData,
    last_advanced: Option<SimpleBlockData>,
) -> Result<Vec<H256>> {
    let (last_advanced_hash, last_advanced_height) = if let Some(la) = last_advanced {
        (la.hash, la.header.height)
    } else {
        let start_eb_hash = db.globals().start_block_hash;
        let genesis_eb_hash = db.config().genesis_block_hash;
        if start_eb_hash != genesis_eb_hash {
            return Err(ComputeError::Other(
                "if last advanced EB is zero, then start block must match genesis",
            ));
        }
        db.block_simple_data(start_eb_hash)
            .ok_or(ComputeError::BlockNotSynced(start_eb_hash))
            .map(|start_eb| {
                start_eb
                    .header
                    .height
                    .checked_sub(1)
                    .ok_or(ComputeError::Other("start block height=0 isn't expected"))
                    .map(|h| (H256::zero(), h))
            })??
    };

    let depth = target
        .header
        .height
        .checked_sub(last_advanced_height)
        .ok_or(ComputeError::Other("target EB is older than last advanced"))?;

    if depth == 0 {
        return Err(ComputeError::Other(
            "target EB is at the same height as last advanced",
        ));
    }

    let mut chain = Vec::with_capacity(depth as usize);
    let mut current = target;
    for step in 0..depth {
        chain.push(current.hash);

        // The deepest block's parent must connect to the last advanced EB. Its
        // parent header is not fetched: for the genesis EB that parent is the
        // un-seeded zero hash, so we compare the `parent_hash` field directly.
        let parent_hash = current.header.parent_hash;
        if step + 1 == depth {
            if parent_hash != last_advanced_hash {
                return Err(ComputeError::Other(
                    "collected advancing chain does not connect to the last advanced block",
                ));
            }
            break;
        }

        current = db
            .block_simple_data(parent_hash)
            .ok_or(ComputeError::Other(
                "block header not found while collecting advancing chain",
            ))?;
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
        // (1) Pick up the next ready request whenever no work is in flight.
        // Skip already-computed requests and park ones whose prerequisite EB
        // is not prepared yet into `deferred`, scanning past them so a parked
        // request never head-of-line blocks a ready one behind it.
        if self.computation.is_none()
            && self.promises_stream.is_none()
            && self.pending_event.is_none()
        {
            while let Some(req) = self.input.pop_front() {
                if self.db.mb_meta(req.mb_hash).computed {
                    continue;
                }
                if !self.prerequisite_ready(req.mb_hash) {
                    self.deferred.push_back(req);
                    continue;
                }
                let (sender, receiver) = mpsc::unbounded_channel();
                self.in_flight_mb = Some(req.mb_hash);
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
                break;
            }
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
            self.in_flight_mb = None;
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
    use crate::tests::{MockProcessor, proptest_config};
    use ethexe_common::{
        BlockHeader, CodeAndIdUnchecked, DEFAULT_BLOCK_GAS_LIMIT, PrivateKey, SignedMessage,
        db::*,
        events::{
            BlockEvent, MirrorEvent, RouterEvent,
            mirror::{ExecutableBalanceTopUpRequestedEvent, MessageQueueingRequestedEvent},
            router::ProgramCreatedEvent,
        },
        injected::{InjectedTransaction, SignedInjectedTransaction},
        mock::seed_genesis_zero_mb,
    };
    use ethexe_processor::{Processor, ValidCodeInfo};
    use ethexe_runtime_common::RUNTIME_ID;
    use gear_core::ids::prelude::CodeIdExt;
    use gprimitives::{ActorId, CodeId, MessageId};
    use proptest::prelude::*;

    fn eb_hash(height: u32) -> H256 {
        H256::from_low_u64_be(0xEB00 + height as u64)
    }

    /// Synthetic Ethereum block at `height`, chained onto its height-1 parent
    /// (zero parent at height 1 — the genesis Eth block). The hash is derived
    /// from the height, so a contiguous chain builds itself.
    fn synthetic_eb(db: &Database, height: u32, events: Vec<BlockEvent>) -> H256 {
        let parent_hash = if height <= 1 {
            H256::zero()
        } else {
            eb_hash(height - 1)
        };
        let hash = eb_hash(height);
        db.set_block_header(
            hash,
            BlockHeader {
                height,
                timestamp: height as u64,
                parent_hash,
            },
        );
        db.set_block_events(hash, &events);
        // Compute now defers an MB until the EB it advances to is prepared, so
        // these synthetic EBs (standing in for already-synced blocks) must carry
        // the flag for the direct `ComputeSubService` tests that never pump a
        // `BlockPrepared` notification.
        db.mutate_block_meta(hash, |m| m.prepared = true);
        hash
    }

    /// Seed the genesis Eth block (height 1) and point the genesis zero-MB's
    /// `last_advanced_eb` at it, so subsequent MBs walk a real depth-1 chain
    /// via the `Some` branch — exactly as the malachite service propagates it.
    fn seed_genesis_eth(db: &Database) -> H256 {
        let gen_eb = synthetic_eb(db, 1, vec![]);
        db.mutate_mb_meta(H256::zero(), |m| m.last_advanced_eb = gen_eb);
        gen_eb
    }

    fn dummy_ops(db: &Database, eb_height: u32) -> Operations {
        // The unique EB height makes each MB's operations list (and thus its
        // CAS hash) unique, and gives the advance walk a chained block to pick up.
        let eth_block_hash = synthetic_eb(db, eb_height, vec![]);
        Operations::new(vec![
            Operation::AdvanceTillEthereumBlock {
                block_hash: eth_block_hash,
            },
            Operation::ProgressTasks,
            Operation::ProcessQueuesV3 {
                gas_allowance: DEFAULT_BLOCK_GAS_LIMIT,
            },
        ])
    }

    /// Mimics malachite `process_mb_proposal`: CAS write + `CompactMb`.
    fn seed_mb(db: &Database, mb_hash: H256, parent: H256, height: u64, ops: Operations) {
        let operations_hash = db.set_operations(ops);
        db.set_mb_compact_block(
            mb_hash,
            CompactMb {
                parent,
                height,
                operations_hash,
            },
        );
    }

    /// `seed_mb` plus the malachite-side bookkeeping: record the advanced EB
    /// as this MB's `last_advanced_eb`, so its child walks a depth-1 chain.
    fn seed_mb_advancing(db: &Database, mb_hash: H256, parent: H256, height: u64, eb_height: u32) {
        seed_mb(db, mb_hash, parent, height, dummy_ops(db, eb_height));
        db.mutate_mb_meta(mb_hash, |m| m.last_advanced_eb = eb_hash(eb_height));
    }

    /// Tail-only queue still computes all uncomputed predecessors.
    #[tokio::test]
    #[ntest::timeout(5000)]
    async fn walks_uncomputed_predecessors() {
        gear_utils::init_default_logger();

        let db = Database::memory();
        seed_genesis_zero_mb(&db);
        seed_genesis_eth(&db);
        let processor = MockProcessor::default();
        let mut sub = ComputeSubService::new(db.clone(), processor);

        // 5-block chain; mb_hash = 0x1000 + i. Each MB advances one EB, so
        // MB at height i pins the EB at height i + 1 (genesis Eth is height 1).
        const N: u64 = 5;
        let mut hashes = Vec::with_capacity(N as usize);
        let mut parent = H256::zero();
        for i in 1..=N {
            let mb_hash = H256::from_low_u64_be(0x1000 + i);
            seed_mb_advancing(&db, mb_hash, parent, i, (i + 1) as u32);
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

    proptest! {
        #![proptest_config(proptest_config(64))]

        #[test]
        fn collect_uncomputed_chain_returns_oldest_first(chain_len in 2u64..=16) {
            let db = Database::memory();
            let mut hashes = Vec::with_capacity(chain_len as usize);
            let mut parent = H256::zero();

            for i in 1..=chain_len {
                let mb_hash = H256::from_low_u64_be(0xB000 + i);
                seed_mb(&db, mb_hash, parent, i, dummy_ops(&db, (i + 1) as u32));
                hashes.push(mb_hash);
                parent = mb_hash;
            }

            db.mutate_mb_meta(hashes[0], |meta| {
                meta.computed = true;
            });

            let collected = collect_uncomputed_chain(&db, *hashes.last().unwrap())
                .unwrap()
                .into_iter()
                .map(|(mb_hash, _)| mb_hash)
                .collect::<Vec<_>>();

            prop_assert_eq!(collected, hashes[1..].to_vec());
        }
    }

    /// `collect_advance_chain` must surface a missing intermediate header
    /// instead of silently truncating the chain. A partial walk would let
    /// validators with different DB completeness emit different advance
    /// events for the same MB — a determinism break.
    #[test]
    fn collect_advance_chain_errors_on_missing_intermediate_header() {
        let db = Database::memory();
        let last_advanced = SimpleBlockData {
            hash: H256::from_low_u64_be(0xA0),
            header: BlockHeader {
                height: 0,
                timestamp: 0,
                parent_hash: H256::zero(),
            },
        };
        let parent_b = H256::from_low_u64_be(0xA1);
        let parent_a = H256::from_low_u64_be(0xA2);
        let target_hash = H256::from_low_u64_be(0xA3);

        // target -> parent_a -> parent_b -> last_advanced
        // parent_b's header is intentionally missing.
        db.set_block_header(
            target_hash,
            BlockHeader {
                height: 3,
                timestamp: 3,
                parent_hash: parent_a,
            },
        );
        db.set_block_header(
            parent_a,
            BlockHeader {
                height: 2,
                timestamp: 2,
                parent_hash: parent_b,
            },
        );

        let target = db.block_simple_data(target_hash).unwrap();
        let result = collect_advance_chain(&db, target, Some(last_advanced));
        match result {
            Err(ComputeError::Other(msg)) => assert!(
                msg.contains("block header not found"),
                "expected a missing-header error for {parent_b:?}, got message: {msg:?} — \
                 a silent truncation here would non-determinise event replay across peers"
            ),
            other => panic!(
                "expected a missing-header error for {parent_b:?}, got {other:?} — \
                 a silent truncation here would non-determinise event replay across peers"
            ),
        }
    }

    /// Re-queueing an already-computed MB is a no-op: receive_mb
    /// drops the request before it ever reaches `compute`, so the
    /// stream emits nothing (preventing a duplicate `MbComputed`
    /// when both `BlockProposal` and `BlockFinalized` queue the
    /// same head).
    #[tokio::test]
    #[ntest::timeout(5000)]
    async fn idempotent_for_computed_target() {
        let db = Database::memory();
        let processor = MockProcessor::default();
        let mut sub = ComputeSubService::new(db.clone(), processor);

        let mb_hash = H256::from_low_u64_be(0xCAFE);
        seed_mb(&db, mb_hash, H256::zero(), 1, dummy_ops(&db, 2));
        db.mutate_mb_meta(mb_hash, |meta| {
            meta.computed = true;
        });

        sub.receive_mb(mb_hash, ::ethexe_common::PromisePolicy::Enabled);

        let result = tokio::time::timeout(std::time::Duration::from_millis(100), sub.next()).await;
        assert!(
            result.is_err(),
            "stream must stay pending — re-queue of computed MB is a no-op"
        );
    }

    // --- Promise emission-mode tests (real Processor + demo-ping) ---
    //
    // `compute_mb` walks back uncomputed ancestor MBs and runs them
    // oldest-first. Which of those MBs surface `ComputeEvent::Promise`s
    // depends on the sub-service's `PromiseEmissionMode`:
    //   * `ConsensusDriven` — only the directly requested head emits,
    //     and only when the caller passes `PromisePolicy::Enabled`.
    //   * `AlwaysEmit` — every MB in the walked chain emits, so an RPC
    //     node catching up still surfaces replies for predecessors.

    async fn upload_ping_code(processor: &mut Processor, db: &Database) -> CodeId {
        let code = demo_ping::WASM_BINARY;
        let code_id = CodeId::generate(code);
        let ValidCodeInfo {
            code,
            instrumented_code,
            code_metadata,
        } = processor
            .process_code(CodeAndIdUnchecked {
                code: code.to_vec(),
                code_id,
            })
            .await
            .expect("failed to process demo-ping code")
            .valid
            .expect("demo-ping code is invalid");
        db.set_original_code(&code);
        db.set_instrumented_code(RUNTIME_ID, code_id, instrumented_code);
        db.set_code_metadata(code_id, code_metadata);
        db.set_code_valid(code_id, true);
        code_id
    }

    fn ping_injected(destination: ActorId) -> SignedInjectedTransaction {
        let tx = InjectedTransaction {
            destination,
            payload: b"PING".to_vec().try_into().unwrap(),
            value: 0,
            reference_block: H256::random(),
            salt: H256::random().0.to_vec().try_into().unwrap(),
        };
        SignedMessage::create(PrivateKey::random(), tx).expect("failed to sign injected tx")
    }

    fn mb_bookend() -> [Operation; 2] {
        [
            Operation::ProgressTasks,
            Operation::ProcessQueuesV3 {
                gas_allowance: DEFAULT_BLOCK_GAS_LIMIT,
            },
        ]
    }

    /// MB #0 creates + funds a demo-ping program; each later MB injects
    /// one `PING`. Returns the MB hashes, head last.
    async fn build_ping_mb_chain(
        db: &Database,
        processor: &mut Processor,
        pinger_count: u64,
    ) -> Vec<H256> {
        let ping_code_id = upload_ping_code(processor, db).await;
        let ping_id = ActorId::from(0x10000);

        let mut mb_hashes = Vec::new();

        // MB #0 — create + fund + initialize the ping program via an
        // Ethereum block. The canonical init message is required: an
        // injected transaction cannot target an uninitialized program.
        // EBs are chained onto the genesis Eth block (height 1), so the
        // create block is height 2 and each pinger advances one more EB.
        let create_eb = synthetic_eb(
            db,
            2,
            vec![
                BlockEvent::Router(RouterEvent::ProgramCreated(ProgramCreatedEvent {
                    actor_id: ping_id,
                    code_id: ping_code_id,
                })),
                BlockEvent::Mirror {
                    actor_id: ping_id,
                    event: MirrorEvent::ExecutableBalanceTopUpRequested(
                        ExecutableBalanceTopUpRequestedEvent {
                            value: 500_000_000_000_000,
                        },
                    ),
                },
                BlockEvent::Mirror {
                    actor_id: ping_id,
                    event: MirrorEvent::MessageQueueingRequested(MessageQueueingRequestedEvent {
                        id: MessageId::new(H256::random().0),
                        source: ActorId::from(0xa11ce),
                        payload: b"PING".to_vec(),
                        value: 0,
                        call_reply: false,
                    }),
                },
            ],
        );
        let creator = H256::from_low_u64_be(0x1000);
        let mut ops = vec![Operation::AdvanceTillEthereumBlock {
            block_hash: create_eb,
        }];
        ops.extend(mb_bookend());
        seed_mb(db, creator, H256::zero(), 0, Operations::new(ops));
        db.mutate_mb_meta(creator, |m| m.last_advanced_eb = create_eb);
        mb_hashes.push(creator);

        // MB #1.. — each injects a single PING into the ping program.
        for i in 1..=pinger_count {
            let eb_height = 2 + i as u32;
            let eb = synthetic_eb(db, eb_height, vec![]);
            let mb_hash = H256::from_low_u64_be(0x1000 + i);
            let mut ops = vec![
                Operation::AdvanceTillEthereumBlock { block_hash: eb },
                Operation::Injected(ping_injected(ping_id)),
            ];
            ops.extend(mb_bookend());
            seed_mb(
                db,
                mb_hash,
                *mb_hashes.last().unwrap(),
                i,
                Operations::new(ops),
            );
            db.mutate_mb_meta(mb_hash, |m| m.last_advanced_eb = eb);
            mb_hashes.push(mb_hash);
        }

        mb_hashes
    }

    /// Computes the chain head and returns `(mb_hashes, promises)` where
    /// each promise is paired with the MB hash that produced it.
    async fn run_emission(
        mode: PromiseEmissionMode,
        policy: PromisePolicy,
        pinger_count: u64,
    ) -> (Vec<H256>, Vec<(H256, Promise)>) {
        let db = Database::memory();
        seed_genesis_zero_mb(&db);
        seed_genesis_eth(&db);
        let mut processor = Processor::new(db.clone()).expect("failed to create processor");
        let mb_hashes = build_ping_mb_chain(&db, &mut processor, pinger_count).await;

        let mut sub = ComputeSubService::with_promise_mode(db.clone(), processor, mode);
        let head = *mb_hashes.last().unwrap();
        sub.receive_mb(head, policy);

        let mut promises = Vec::new();
        loop {
            match sub.next().await.expect("compute sub-service event") {
                ComputeEvent::Promise(promise, mb_hash) => promises.push((mb_hash, promise)),
                ComputeEvent::MbComputed(hash) => {
                    assert_eq!(hash, head, "MbComputed must report the requested head");
                    break;
                }
                other => panic!("unexpected compute event: {other:?}"),
            }
        }
        (mb_hashes, promises)
    }

    /// `ConsensusDriven`: only the directly requested head MB emits a
    /// promise — the parent-walked predecessors stay silent even though
    /// each of them also carries an injected `PING`.
    #[tokio::test]
    #[ntest::timeout(60000)]
    async fn consensus_driven_emits_only_head_mb() {
        gear_utils::init_default_logger();

        let (mb_hashes, promises) = run_emission(
            PromiseEmissionMode::ConsensusDriven,
            PromisePolicy::Enabled,
            3,
        )
        .await;

        let head = *mb_hashes.last().unwrap();
        let emitting: Vec<H256> = promises.iter().map(|(mb, _)| *mb).collect();
        assert_eq!(
            emitting,
            vec![head],
            "ConsensusDriven must emit promises only for the requested head MB"
        );
        assert_eq!(promises[0].1.reply.payload, *b"PONG");
    }

    /// `AlwaysEmit`: every walked MB emits a promise, predecessors
    /// included — and it does so regardless of the per-MB `PromisePolicy`.
    #[tokio::test]
    #[ntest::timeout(60000)]
    async fn always_emit_emits_every_walked_mb() {
        gear_utils::init_default_logger();

        let (mb_hashes, promises) =
            run_emission(PromiseEmissionMode::AlwaysEmit, PromisePolicy::Disabled, 3).await;

        // mb_hashes[0] creates the program (no injected tx); the three
        // pingers each produce one promise, in oldest-first order.
        let expected: Vec<H256> = mb_hashes[1..].to_vec();
        let emitting: Vec<H256> = promises.iter().map(|(mb, _)| *mb).collect();
        assert_eq!(
            emitting, expected,
            "AlwaysEmit must surface a promise for every MB in the walked chain"
        );
        for (_, promise) in &promises {
            assert_eq!(promise.reply.payload, *b"PONG");
        }
    }
}
