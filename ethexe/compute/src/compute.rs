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

use crate::{ComputeError, ComputeEvent, ProcessorExt, Result, service::SubService};
use ethexe_common::{
    Announce, HashOf, PromiseEmissionMode, PromisePolicy, SimpleBlockData,
    db::{
        AnnounceStorageRO, AnnounceStorageRW, BlockMetaStorageRO, CodesStorageRW, ConfigStorageRO,
        GlobalsStorageRW, OnChainStorageRO,
    },
    events::BlockEvent,
    injected::Promise,
};
use ethexe_db::Database;
use ethexe_processor::ExecutableData;
use ethexe_runtime_common::FinalizedBlockTransitions;
use futures::{FutureExt, StreamExt, future::BoxFuture};
use gprimitives::H256;
use std::{
    collections::VecDeque,
    task::{Context, Poll},
};
use tokio::sync::mpsc;

/// Metrics for the [`ComputeSubService`].
#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_compute_compute")]
struct Metrics {
    /// The latency of announce processing in seconds represented as f64.
    announce_processing_latency: metrics::Histogram,
}

/// Configuration for [ComputeSubService].
#[derive(Debug, Clone, Copy, bon::Builder)]
#[cfg_attr(test, derive(Default))]
pub struct ComputeConfig {
    /// The delay in **blocks** in which events from Ethereum will be apply.
    canonical_quarantine: u8,
    /// The promises emission rule.
    promises_mode: PromiseEmissionMode,
}

impl ComputeConfig {
    pub fn canonical_quarantine(&self) -> u8 {
        self.canonical_quarantine
    }

    pub fn promises_mode(&self) -> PromiseEmissionMode {
        self.promises_mode
    }
}

/// Type alias for computation future with timing.
type ComputationFuture = future_timing::Timed<BoxFuture<'static, Result<HashOf<Announce>>>>;

pub struct ComputeSubService<P: ProcessorExt> {
    db: Database,
    processor: P,
    config: ComputeConfig,
    metrics: Metrics,

    input: VecDeque<(Announce, PromisePolicy)>,

    // TODO kuzmindev: consider to refactor this (move to separate stream).
    computation: Option<ComputationFuture>,
    promises_stream: Option<utils::AnnouncePromisesStream>,
    pending_event: Option<Result<ComputeEvent>>,
}

impl<P: ProcessorExt> ComputeSubService<P> {
    pub fn new(config: ComputeConfig, db: Database, processor: P) -> Self {
        Self {
            db,
            processor,
            config,
            metrics: Metrics::default(),
            input: VecDeque::new(),
            computation: None,
            promises_stream: None,
            pending_event: None,
        }
    }

    pub fn receive_announce_to_compute(
        &mut self,
        announce: Announce,
        promise_policy: PromisePolicy,
    ) {
        self.input.push_back((announce, promise_policy));
    }

    async fn compute(
        db: Database,
        config: ComputeConfig,
        mut processor: P,
        announce: Announce,
        promise_out_tx: Option<mpsc::UnboundedSender<Promise>>,
    ) -> Result<HashOf<Announce>> {
        let announce_hash = announce.to_hash();
        let block_hash = announce.block_hash;

        if !db.block_meta(block_hash).prepared {
            return Err(ComputeError::BlockNotPrepared(block_hash));
        }

        let not_computed_announces = utils::collect_not_computed_predecessors(&announce, &db)?;
        if !not_computed_announces.is_empty() {
            log::trace!(
                "compute-sub-service: announce({announce_hash}) contains a {} previous not computed announce, start computing...",
                not_computed_announces.len(),
            );

            let promise_tx = match config.promises_mode() {
                // If AlwaysEmit promises mode - we pass promises tx also for not computed chain.
                PromiseEmissionMode::AlwaysEmit => promise_out_tx.clone(),
                // Set the promise_out_tx = None, because in this case we want to receive promises only from target announce.
                PromiseEmissionMode::ConsensusDriven => None,
            };

            for (announce_hash, announce) in not_computed_announces {
                Self::compute_one(
                    &db,
                    &mut processor,
                    config,
                    announce_hash,
                    announce,
                    promise_tx.clone(),
                )
                .await?;
            }
        }

        // Compute the target announce
        Self::compute_one(
            &db,
            &mut processor,
            config,
            announce_hash,
            announce,
            promise_out_tx,
        )
        .await
    }

    async fn compute_one(
        db: &Database,
        processor: &mut P,
        config: ComputeConfig,
        announce_hash: HashOf<Announce>,
        announce: Announce,
        promise_out_tx: Option<mpsc::UnboundedSender<Promise>>,
    ) -> Result<HashOf<Announce>> {
        let executable =
            utils::prepare_executable_for_announce(db, announce, config.canonical_quarantine())?;
        let processing_result = processor
            .process_programs(executable, promise_out_tx)
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

        db.set_announce_outcome(announce_hash, transitions);
        db.set_announce_program_states(announce_hash, states);
        db.set_announce_schedule(announce_hash, schedule);
        db.mutate_announce_meta(announce_hash, |meta| {
            meta.computed = true;
        });

        db.globals_mutate(|globals| {
            globals.latest_computed_announce_hash = announce_hash;
        });

        Ok(announce_hash)
    }
}

impl<P: ProcessorExt> SubService for ComputeSubService<P> {
    type Output = ComputeEvent;

    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Output>> {
        if self.computation.is_none()
            && self.promises_stream.is_none()
            && let Some((announce, promise_policy)) = self.input.pop_front()
        {
            let maybe_promise_out_tx =
                match utils::resolve_promise_policy(promise_policy, self.config.promises_mode()) {
                    PromisePolicy::Enabled => {
                        let (sender, receiver) = mpsc::unbounded_channel();
                        self.promises_stream = Some(utils::AnnouncePromisesStream::new(
                            receiver,
                            announce.to_hash(),
                        ));

                        Some(sender)
                    }
                    PromisePolicy::Disabled => None,
                };

            self.computation = Some(future_timing::timed(
                Self::compute(
                    self.db.clone(),
                    self.config,
                    self.processor.clone(),
                    announce,
                    maybe_promise_out_tx,
                )
                .boxed(),
            ));
        }

        if let Some(ref mut stream) = self.promises_stream
            && let Poll::Ready(maybe_event) = stream.poll_next_unpin(cx)
        {
            match maybe_event {
                Some(event) => return Poll::Ready(Ok(event)),
                None => {
                    log::trace!("announce's promises stream is ended");
                    self.promises_stream = None;

                    // Checking for possible event of finishing announce computation.
                    if let Some(event) = self.pending_event.take() {
                        return Poll::Ready(event);
                    }
                }
            }
        }

        if let Some(ref mut computation) = self.computation
            && let Poll::Ready(timing_result) = computation.poll_unpin(cx)
        {
            let (timing, result) = timing_result.into_parts();
            self.metrics
                .announce_processing_latency
                .record((timing.busy() + timing.idle()).as_secs_f64());

            self.computation = None;

            match self.promises_stream.is_some() {
                true => {
                    // We cannot return [`ComputeEvent::AnnounceComputed`] before all promises will be given.
                    self.pending_event = Some(result.map(Into::into));
                }
                false => {
                    return Poll::Ready(result.map(Into::into));
                }
            }
        }

        Poll::Pending
    }
}

/// The utils for [`ComputeSubService`].
pub(crate) mod utils {
    use super::*;
    use futures::Stream;
    use std::pin::Pin;

    /// Resolves [PromisePolicy] with consensus provided policy and global
    /// [PromiseEmissionMode] set for node.
    pub(super) fn resolve_promise_policy(
        consensus_policy: PromisePolicy,
        mode: PromiseEmissionMode,
    ) -> PromisePolicy {
        match mode {
            PromiseEmissionMode::AlwaysEmit => PromisePolicy::Enabled,
            PromiseEmissionMode::ConsensusDriven => consensus_policy,
        }
    }

    /// The stream of promises from announce execution.
    pub(super) struct AnnouncePromisesStream {
        receiver: mpsc::UnboundedReceiver<Promise>,
        announce_hash: HashOf<Announce>,
    }

    impl AnnouncePromisesStream {
        pub fn new(
            receiver: mpsc::UnboundedReceiver<Promise>,
            announce_hash: HashOf<Announce>,
        ) -> Self {
            Self {
                receiver,
                announce_hash,
            }
        }
    }

    impl Stream for AnnouncePromisesStream {
        type Item = ComputeEvent;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(
                futures::ready!(self.receiver.poll_recv(cx))
                    .map(|promise| ComputeEvent::Promise(promise, self.announce_hash)),
            )
        }
    }

    pub fn prepare_executable_for_announce(
        db: &Database,
        announce: Announce,
        canonical_quarantine: u8,
    ) -> Result<ExecutableData> {
        let block_hash = announce.block_hash;

        let matured_events =
            find_canonical_events_post_quarantine(db, block_hash, canonical_quarantine)?;

        let events = matured_events
            .into_iter()
            .filter_map(|event| event.to_request())
            .collect();

        Ok(ExecutableData {
            block: SimpleBlockData {
                hash: block_hash,
                header: db
                    .block_header(block_hash)
                    .ok_or(ComputeError::BlockHeaderNotFound(block_hash))?,
            },
            program_states: db
                .announce_program_states(announce.parent)
                .ok_or(ComputeError::ProgramStatesNotFound(announce.parent))?,
            schedule: db
                .announce_schedule(announce.parent)
                .ok_or(ComputeError::ScheduleNotFound(announce.parent))?,
            injected_transactions: announce
                .injected_transactions
                .into_iter()
                .map(|tx| tx.into_verified())
                .collect(),
            gas_allowance: announce.gas_allowance,
            events,
        })
    }

    pub(super) fn collect_not_computed_predecessors<DB>(
        announce: &Announce,
        db: &DB,
    ) -> Result<VecDeque<(HashOf<Announce>, Announce)>>
    where
        DB: AnnounceStorageRO,
    {
        let mut parent_hash = announce.parent;
        let mut announces_chain = VecDeque::new();

        loop {
            if db.announce_meta(parent_hash).computed {
                break;
            }

            let parent_announce = db
                .announce(parent_hash)
                .ok_or(ComputeError::AnnounceNotFound(parent_hash))?;

            let next_parent_hash = parent_announce.parent;
            announces_chain.push_front((parent_hash, parent_announce));

            parent_hash = next_parent_hash;
        }

        Ok(announces_chain)
    }

    /// Finds events from Ethereum in database which can be processed in current block.
    pub fn find_canonical_events_post_quarantine(
        db: &Database,
        mut block_hash: H256,
        canonical_quarantine: u8,
    ) -> Result<Vec<BlockEvent>> {
        let genesis_block = db.config().genesis_block_hash;

        let mut block_header = db
            .block_header(block_hash)
            .ok_or_else(|| ComputeError::BlockHeaderNotFound(block_hash))?;

        for _ in 0..canonical_quarantine {
            if block_hash == genesis_block {
                return Ok(Default::default());
            }

            let parent_hash = block_header.parent_hash;
            let parent_header = db
                .block_header(parent_hash)
                .ok_or(ComputeError::BlockHeaderNotFound(parent_hash))?;

            block_hash = parent_hash;
            block_header = parent_header;
        }

        db.block_events(block_hash)
            .ok_or(ComputeError::BlockEventsNotFound(block_hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ComputeService, tests::MockProcessor};
    use ethexe_common::{
        DEFAULT_BLOCK_GAS_LIMIT,
        db::{GlobalsStorageRO, OnChainStorageRW},
        events::{
            RouterEvent, mirror::ExecutableBalanceTopUpRequestedEvent, router::ProgramCreatedEvent,
        },
        gear::StateTransition,
        mock::*,
    };
    use ethexe_processor::Processor;
    use gear_core::{
        message::{ReplyCode, SuccessReplyReason},
        rpc::ReplyInfo,
    };
    use gprimitives::{ActorId, H256};

    mod test_utils {
        use crate::CodeAndIdUnchecked;
        use ethexe_common::{
            PrivateKey, SignedMessage,
            events::{MirrorEvent, mirror::MessageQueueingRequestedEvent},
            injected::{InjectedTransaction, SignedInjectedTransaction},
        };
        use ethexe_processor::ValidCodeInfo;
        use ethexe_runtime_common::RUNTIME_ID;
        use gear_core::ids::prelude::CodeIdExt;
        use gprimitives::{CodeId, MessageId};

        use super::*;

        const USER_ID: ActorId = ActorId::new([1u8; 32]);

        pub fn upload_code(processor: &mut Processor, code: &[u8], db: &Database) -> CodeId {
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
                .expect("failed to process code")
                .valid
                .expect("code is invalid");

            db.set_original_code(&code);
            db.set_instrumented_code(RUNTIME_ID, code_id, instrumented_code);
            db.set_code_metadata(code_id, code_metadata);
            db.set_code_valid(code_id, true);

            code_id
        }

        pub fn block_events(len: usize, actor_id: ActorId, payload: Vec<u8>) -> Vec<BlockEvent> {
            (0..len)
                .map(|_| canonical_event(actor_id, payload.clone()))
                .collect()
        }

        pub fn canonical_event(actor_id: ActorId, payload: Vec<u8>) -> BlockEvent {
            BlockEvent::Mirror {
                actor_id,
                event: MirrorEvent::MessageQueueingRequested(MessageQueueingRequestedEvent {
                    id: MessageId::new(H256::random().0),
                    source: USER_ID,
                    value: 0,
                    payload,
                    call_reply: false,
                }),
            }
        }

        pub fn create_program_events(actor_id: ActorId, code_id: CodeId) -> Vec<BlockEvent> {
            let created_event =
                BlockEvent::Router(RouterEvent::ProgramCreated(ProgramCreatedEvent {
                    actor_id,
                    code_id,
                }));

            let top_up_event = BlockEvent::Mirror {
                actor_id,
                event: MirrorEvent::ExecutableBalanceTopUpRequested(
                    ExecutableBalanceTopUpRequestedEvent {
                        value: 500_000_000_000_000,
                    },
                ),
            };

            vec![created_event, top_up_event]
        }

        pub fn injected_tx(
            destination: ActorId,
            payload: Vec<u8>,
            ref_block: H256,
        ) -> SignedInjectedTransaction {
            let tx = InjectedTransaction {
                destination,
                payload: payload.try_into().unwrap(),
                value: 0,
                reference_block: ref_block,
                salt: H256::random().0.to_vec().try_into().unwrap(),
            };
            let pk = PrivateKey::random();
            SignedMessage::create(pk, tx).unwrap()
        }
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn test_compute() {
        gear_utils::init_default_logger();

        // Create non-empty processor result with transitions
        let non_empty_result = FinalizedBlockTransitions {
            transitions: vec![StateTransition {
                actor_id: ActorId::from([1; 32]),
                new_state_hash: H256::from([2; 32]),
                value_to_receive: 100,
                ..Default::default()
            }],
            ..Default::default()
        };

        let db = Database::memory();
        let block_hash = BlockChain::mock(1).setup(&db).blocks[1].hash;
        let config = ComputeConfig::default();
        let mut service = ComputeSubService::new(
            config,
            db.clone(),
            MockProcessor {
                process_programs_result: Some(non_empty_result),
                ..Default::default()
            },
        );

        let announce = Announce {
            block_hash,
            parent: db.config().genesis_announce_hash,
            gas_allowance: Some(100),
            injected_transactions: vec![],
        };
        let announce_hash = announce.to_hash();

        service.receive_announce_to_compute(announce, PromisePolicy::Disabled);

        assert_eq!(
            service.next().await.unwrap().unwrap_announce_computed(),
            announce_hash
        );

        // Verify block was marked as computed
        assert!(db.announce_meta(announce_hash).computed);

        // Verify transitions were stored in DB
        let stored_transitions = db.announce_outcome(announce_hash).unwrap();
        assert_eq!(stored_transitions.len(), 1);
        assert_eq!(stored_transitions[0].actor_id, ActorId::from([1; 32]));
        assert_eq!(stored_transitions[0].new_state_hash, H256::from([2; 32]));

        // Verify latest announce
        assert_eq!(db.globals().latest_computed_announce_hash, announce_hash);
    }

    #[tokio::test]
    #[ntest::timeout(60000)]
    async fn test_compute_with_promises() {
        gear_utils::init_default_logger();
        const BLOCKCHAIN_LEN: usize = 10;

        let db = Database::memory();
        let mut processor = Processor::new(db.clone()).unwrap();
        let ping_code_id = test_utils::upload_code(&mut processor, demo_ping::WASM_BINARY, &db);
        let ping_id = ActorId::from(0x10000);

        let blockchain = BlockChain::mock(BLOCKCHAIN_LEN as u32).setup(&db);

        // Setup first announce.
        let start_announce_hash = {
            let mut announce = blockchain.block_top_announce(0).announce.clone();
            announce.gas_allowance = Some(DEFAULT_BLOCK_GAS_LIMIT);

            let announce_hash = db.set_announce(announce);
            db.mutate_announce_meta(announce_hash, |meta| meta.computed = true);
            db.globals_mutate(|globals| {
                globals.start_announce_hash = announce_hash;
            });
            db.set_announce_program_states(announce_hash, Default::default());
            db.set_announce_schedule(announce_hash, Default::default());

            announce_hash
        };

        // Setup announces and events.
        let mut parent_announce = start_announce_hash;
        let announces_chain = (1..BLOCKCHAIN_LEN)
            .map(|i| {
                let announce = {
                    let mut announce = blockchain.block_top_announce(i).announce.clone();
                    announce.gas_allowance = Some(DEFAULT_BLOCK_GAS_LIMIT);
                    announce.parent = parent_announce;

                    let block = announce.block_hash;
                    let txs = if i != 1 {
                        vec![test_utils::injected_tx(ping_id, b"PING".into(), block)]
                    } else {
                        Default::default()
                    };

                    announce.injected_transactions = txs;
                    announce
                };

                let announce_hash = db.set_announce(announce.clone());
                db.mutate_announce_meta(announce_hash, |meta| meta.computed = false);

                let mut block_events = if i == 1 {
                    test_utils::create_program_events(ping_id, ping_code_id)
                } else {
                    Default::default()
                };
                block_events.extend(test_utils::block_events(5, ping_id, b"PING".into()));
                db.set_block_events(announce.block_hash, &block_events);

                parent_announce = announce_hash;
                announce
            })
            .collect::<Vec<_>>();

        let mut compute_service =
            ComputeService::new(ComputeConfig::default(), db.clone(), processor);

        // Send announces for computation.
        compute_service.compute_announce(
            announces_chain.get(2).unwrap().clone(),
            PromisePolicy::Enabled,
        );
        compute_service.compute_announce(
            announces_chain.get(5).unwrap().clone(),
            PromisePolicy::Enabled,
        );
        compute_service.compute_announce(
            announces_chain.get(8).unwrap().clone(),
            PromisePolicy::Enabled,
        );

        let mut expected_announces = vec![
            announces_chain.get(2).unwrap().to_hash(),
            announces_chain.get(5).unwrap().to_hash(),
            announces_chain.get(8).unwrap().to_hash(),
        ];

        let mut expected_promises = expected_announces
            .iter()
            .map(|hash| {
                let announce = db.announce(*hash).unwrap();
                let tx = announce.injected_transactions[0].clone().into_data();
                Promise {
                    tx_hash: tx.to_hash(),
                    reply: ReplyInfo {
                        payload: b"PONG".into(),
                        value: 0,
                        code: ReplyCode::Success(SuccessReplyReason::Manual),
                    },
                }
            })
            .collect::<Vec<_>>();

        while !expected_announces.is_empty() || !expected_promises.is_empty() {
            match compute_service.next().await.unwrap().unwrap() {
                ComputeEvent::AnnounceComputed(hash) => {
                    if *expected_announces.first().unwrap() == hash {
                        expected_announces.remove(0);
                    }
                }
                ComputeEvent::Promise(promise, announce) => {
                    if *expected_announces.first().unwrap() == announce
                        && expected_promises.first().unwrap().clone() == promise
                    {
                        expected_promises.remove(0);
                    }
                }
                _ => unreachable!("unexpected event for current test"),
            }
        }
    }

    #[tokio::test]
    #[ntest::timeout(60000)]
    async fn test_compute_with_early_break() {
        gear_utils::init_default_logger();

        let db = Database::memory();
        let mut processor = Processor::new(db.clone()).unwrap();

        let ping_code_id = test_utils::upload_code(&mut processor, demo_ping::WASM_BINARY, &db);
        let ping_id = ActorId::from(0x10000);

        let blockchain = BlockChain::mock(3).setup(&db);

        let first_announce_hash = {
            let mut announce = blockchain.block_top_announce(1).announce.clone();
            announce.gas_allowance = Some(DEFAULT_BLOCK_GAS_LIMIT);

            let mut canonical_events = test_utils::create_program_events(ping_id, ping_code_id);
            canonical_events.push(test_utils::canonical_event(ping_id, b"PING".into()));

            db.set_block_events(announce.block_hash, &canonical_events);
            db.set_announce(announce)
        };

        let (announce, announce_hash) = {
            let mut announce = blockchain.block_top_announce(2).announce.clone();
            announce.gas_allowance = Some(400_000);
            announce.parent = first_announce_hash;

            let ref_block = announce.block_hash;
            let txs = (0..300)
                .map(|_| test_utils::injected_tx(ping_id, b"PING".into(), ref_block))
                .collect::<Vec<_>>();
            announce.injected_transactions = txs;
            let hash = db.set_announce(announce.clone());
            (announce, hash)
        };

        let mut compute_service =
            ComputeService::new(ComputeConfig::default(), db.clone(), processor);
        compute_service.compute_announce(announce, PromisePolicy::Enabled);

        loop {
            let event = compute_service.next().await.unwrap().unwrap();
            if event == ComputeEvent::AnnounceComputed(announce_hash) {
                break;
            }
        }
    }

    #[test]
    fn collect_not_computed_predecessors_work_correctly() {
        const BLOCKCHAIN_LEN: usize = 10;

        let db = Database::memory();
        let blockchain = BlockChain::mock(BLOCKCHAIN_LEN as u32).setup(&db);

        // Setup announces except the start-announce to not-computed state.
        (0..BLOCKCHAIN_LEN - 1).for_each(|idx| {
            let announce_hash = blockchain.block_top_announce(idx).announce.to_hash();

            if idx == 0 {
                db.mutate_announce_meta(announce_hash, |meta| meta.computed = true);
            } else {
                db.mutate_announce_meta(announce_hash, |meta| meta.computed = false);
            }
        });

        let expected_not_computed_announces = (1..BLOCKCHAIN_LEN - 1)
            .map(|idx| blockchain.block_top_announce(idx).announce.to_hash())
            .collect::<Vec<_>>();

        let head_announce = blockchain
            .block_top_announce(BLOCKCHAIN_LEN - 1)
            .announce
            .clone();
        let not_computed_announces = utils::collect_not_computed_predecessors(&head_announce, &db)
            .unwrap()
            .into_iter()
            .map(|v| v.0)
            .collect::<Vec<_>>();

        assert_eq!(
            expected_not_computed_announces.len(),
            not_computed_announces.len()
        );
        assert_eq!(expected_not_computed_announces, not_computed_announces);
    }
}
