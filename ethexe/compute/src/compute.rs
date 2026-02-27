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
    Announce, HashOf, PromisePolicy, SimpleBlockData,
    db::{
        AnnounceStorageRO, AnnounceStorageRW, BlockMetaStorageRO, CodesStorageRW,
        LatestDataStorageRO, LatestDataStorageRW, OnChainStorageRO,
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

#[derive(Debug, Clone, Copy)]
pub struct ComputeConfig {
    /// The delay in **blocks** in which events from Ethereum will be apply.
    canonical_quarantine: u8,
}

/// Metrics for the [`ComputeSubService`].
#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_compute:compute")]
struct Metrics {
    /// The latency of announce processing in seconds represented as f64.
    announce_processing_latency: metrics::Histogram,
}

impl ComputeConfig {
    /// Constructs [`ComputeConfig`] with provided `canonical_quarantine`.
    /// In production builds `canonical_quarantine` should be equal [`ethexe_common::gear::CANONICAL_QUARANTINE`].
    pub fn new(canonical_quarantine: u8) -> Self {
        Self {
            canonical_quarantine,
        }
    }

    /// Must use only in testing purposes.
    pub fn without_quarantine() -> Self {
        Self {
            canonical_quarantine: 0,
        }
    }

    pub fn canonical_quarantine(&self) -> u8 {
        self.canonical_quarantine
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
    computation: Option<ComputationFuture>,
    promises_stream: Option<utils::AnnouncePromisesStream>,
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

        let not_computed_announces = utils::find_parent_not_computed_announces(&announce, &db)?;

        if !not_computed_announces.is_empty() {
            log::trace!(
                "compute-sub-service: announce({announce_hash}) contains a {} previous not computed announce, start computing...",
                not_computed_announces.len()
            );

            for (announce_hash, announce) in not_computed_announces {
                // Set the promise_out_tx = None, because we want to receive the promises only from target announce.
                Self::compute_one(&db, &mut processor, config, announce_hash, announce, None)
                    .await?;
            }
        }

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
            .process_announce(executable, promise_out_tx)
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

        db.mutate_latest_data(|data| {
            data.computed_announce_hash = announce_hash;
        })
        .ok_or(ComputeError::LatestDataNotFound)?;

        Ok(announce_hash)
    }
}

impl<P: ProcessorExt> SubService for ComputeSubService<P> {
    type Output = ComputeEvent;

    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Output>> {
        if self.computation.is_none()
            && let Some((announce, promise_policy)) = self.input.pop_front()
        {
            let maybe_promise_out_tx = match promise_policy {
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
                    self.promises_stream = None;
                    log::warn!(
                        "compute-sub-service: promises stream shouldn't ended, because the channel can not be dropped, something happen in processor"
                    )
                }
            }
        }

        if let Some(computation) = &mut self.computation
            && let Poll::Ready(timing_result) = computation.poll_unpin(cx)
        {
            let (timing, result) = timing_result.into_parts();
            self.metrics
                .announce_processing_latency
                .record((timing.busy() + timing.idle()).as_secs_f64());

            self.computation = None;
            self.promises_stream = None;

            return Poll::Ready(result.map(Into::into));
        }

        Poll::Pending
    }
}

/// The utils for [`ComputeSubService`].
pub(crate) mod utils {
    use super::*;
    use futures::Stream;
    use std::pin::Pin;

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
            let maybe_promise = futures::ready!(self.receiver.poll_recv(cx));
            Poll::Ready(
                maybe_promise.map(|promise| ComputeEvent::Promise(promise, self.announce_hash)),
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

    pub(super) fn find_parent_not_computed_announces<DB>(
        announce: &Announce,
        db: &DB,
    ) -> Result<VecDeque<(HashOf<Announce>, Announce)>>
    where
        DB: AnnounceStorageRO + LatestDataStorageRO,
    {
        let mut parent_hash = announce.parent;
        let mut announces_chain = VecDeque::new();
        let start_announce_hash = db
            .latest_data()
            .ok_or_else(|| ComputeError::LatestDataNotFound)?
            .start_announce_hash;

        loop {
            if db.announce_meta(parent_hash).computed {
                break;
            }

            let parent_announce = db
                .announce(parent_hash)
                .ok_or(ComputeError::AnnounceNotFound(parent_hash))?;

            let next_parent_hash = parent_announce.parent;
            announces_chain.push_front((parent_hash, parent_announce));

            // This was a start announce, no need to go further.
            if parent_hash == start_announce_hash {
                break;
            }

            parent_hash = next_parent_hash;
        }

        Ok(announces_chain)

        // if announces_chain.is_empty() {
        //     log::trace!("All announces are already computed");
        //     return Ok(announce_hash);
        // }
    }

    /// Finds events from Ethereum in database which can be processed in current block.
    pub fn find_canonical_events_post_quarantine(
        db: &Database,
        mut block_hash: H256,
        canonical_quarantine: u8,
    ) -> Result<Vec<BlockEvent>> {
        let genesis_block = db
            .latest_data()
            .ok_or_else(|| ComputeError::LatestDataNotFound)?
            .genesis_block_hash;

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
    use crate::tests::{MockProcessor, PROCESSOR_RESULT};
    use ethexe_common::{gear::StateTransition, mock::*};
    use gprimitives::{ActorId, H256};

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn test_compute() {
        gear_utils::init_default_logger();

        let db = Database::memory();
        let block_hash = BlockChain::mock(1).setup(&db).blocks[1].hash;
        let config = ComputeConfig::without_quarantine();
        let mut service = ComputeSubService::new(config, db.clone(), MockProcessor);

        let announce = Announce {
            block_hash,
            parent: db.latest_data().unwrap().genesis_announce_hash,
            gas_allowance: Some(100),
            injected_transactions: vec![],
        };
        let announce_hash = announce.to_hash();

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

        // Set the PROCESSOR_RESULT to return non-empty result
        PROCESSOR_RESULT.with_borrow_mut(|r| *r = non_empty_result.clone());
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
        assert_eq!(
            db.latest_data().unwrap().computed_announce_hash,
            announce_hash
        );
    }

    #[test]
    fn find_not_computed_announces_work_correctly() {
        const BLOCKCHAIN_LEN: usize = 10;

        let db = Database::memory();
        let mut blockchain = BlockChain::mock(BLOCKCHAIN_LEN as u32).setup(&db);

        // Setup announces except the head to not-computed state.
        blockchain
            .announces
            .iter_mut()
            .enumerate()
            .for_each(|(idx, (announce_hash, _))| {
                // Set the announces to not computed state
                if idx != BLOCKCHAIN_LEN - 1 {
                    db.mutate_announce_meta(*announce_hash, |meta| {
                        meta.computed = false;
                    });
                }
            });

        let expected_not_computed_announces = (0..BLOCKCHAIN_LEN - 1)
            .map(|idx| blockchain.block_top_announce(idx).announce.to_hash())
            .collect::<Vec<_>>();

        let head_announce = blockchain
            .block_top_announce(BLOCKCHAIN_LEN - 1)
            .announce
            .clone();
        let not_computed_announces = utils::find_parent_not_computed_announces(&head_announce, &db)
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
