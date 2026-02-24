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

use crate::{ComputeError, ProcessorExt, Result, service::SubService};
use ethexe_common::{
    Announce, ComputedAnnounce, HashOf, SimpleBlockData,
    db::{
        AnnounceStorageRO, AnnounceStorageRW, BlockMetaStorageRO, CodesStorageRW,
        LatestDataStorageRO, LatestDataStorageRW, OnChainStorageRO,
    },
    events::BlockEvent,
};
use ethexe_db::Database;
use ethexe_processor::ExecutableData;
use ethexe_runtime_common::FinalizedBlockTransitions;
use futures::future::BoxFuture;
use gprimitives::H256;
use std::{
    collections::VecDeque,
    task::{Context, Poll},
};

#[derive(Debug, Clone, Copy)]
pub struct ComputeConfig {
    /// The delay in **blocks** in which events from Ethereum will be apply.
    canonical_quarantine: u8,
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

pub struct ComputeSubService<P: ProcessorExt> {
    db: Database,
    processor: P,
    config: ComputeConfig,

    input: VecDeque<Announce>,
    computation: Option<BoxFuture<'static, Result<ComputedAnnounce>>>,
}

impl<P: ProcessorExt> ComputeSubService<P> {
    pub fn new(config: ComputeConfig, db: Database, processor: P) -> Self {
        Self {
            db,
            processor,
            config,
            input: VecDeque::new(),
            computation: None,
        }
    }

    pub fn receive_announce_to_compute(&mut self, announce: Announce) {
        self.input.push_back(announce);
    }

    async fn compute(
        db: Database,
        config: ComputeConfig,
        mut processor: P,
        announce: Announce,
    ) -> Result<ComputedAnnounce> {
        let announce_hash = announce.to_hash();
        let block_hash = announce.block_hash;

        if !db.block_meta(block_hash).prepared {
            return Err(ComputeError::BlockNotPrepared(block_hash));
        }

        let mut parent_hash = announce.parent;
        let mut announces_chain: VecDeque<_> = [(announce_hash, announce)].into();
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

        let mut computed_announce = ComputedAnnounce::from_announce_hash(announce_hash);
        if announces_chain.is_empty() {
            log::trace!("All announces are already computed");
            return Ok(computed_announce);
        }

        for (announce_hash, announce) in announces_chain {
            computed_announce.merge_promises(
                Self::compute_one(&db, &mut processor, announce_hash, announce, config).await?,
            );
        }

        Ok(computed_announce)
    }

    async fn compute_one(
        db: &Database,
        processor: &mut P,
        announce_hash: HashOf<Announce>,
        announce: Announce,
        config: ComputeConfig,
    ) -> Result<ComputedAnnounce> {
        let executable =
            prepare_executable_for_announce(db, announce, config.canonical_quarantine())?;
        let processing_result = processor.process_announce(executable).await?;

        let FinalizedBlockTransitions {
            transitions,
            states,
            schedule,
            promises,
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

        Ok(ComputedAnnounce {
            announce_hash,
            promises,
        })
    }
}

impl<P: ProcessorExt> SubService for ComputeSubService<P> {
    type Output = ComputedAnnounce;

    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Output>> {
        if self.computation.is_none()
            && let Some(announce) = self.input.pop_front()
        {
            self.computation = Some(Box::pin(Self::compute(
                self.db.clone(),
                self.config,
                self.processor.clone(),
                announce,
            )));
        }

        if let Some(computation) = &mut self.computation
            && let Poll::Ready(res) = computation.as_mut().poll(cx)
        {
            self.computation = None;
            return Poll::Ready(res);
        }

        Poll::Pending
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

/// Finds events from Ethereum in database which can be processed in current block.
fn find_canonical_events_post_quarantine(
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

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::tests::{MockProcessor, PROCESSOR_RESULT};
//     use ethexe_common::{gear::StateTransition, mock::*};
//     use gprimitives::{ActorId, H256};

//     #[tokio::test]
//     #[ntest::timeout(3000)]
//     async fn test_compute() {
//         gear_utils::init_default_logger();

//         let db = Database::memory();
//         let block_hash = BlockChain::mock(1).setup(&db).blocks[1].hash;
//         let config = ComputeConfig::without_quarantine();
//         let mut service = ComputeSubService::new(config, db.clone(), MockProcessor);

//         let announce = Announce {
//             block_hash,
//             parent: db.latest_data().unwrap().genesis_announce_hash,
//             gas_allowance: Some(100),
//             injected_transactions: vec![],
//         };
//         let announce_hash = announce.to_hash();

//         // Create non-empty processor result with transitions
//         let non_empty_result = FinalizedBlockTransitions {
//             transitions: vec![StateTransition {
//                 actor_id: ActorId::from([1; 32]),
//                 new_state_hash: H256::from([2; 32]),
//                 value_to_receive: 100,
//                 ..Default::default()
//             }],
//             ..Default::default()
//         };

//         // Set the PROCESSOR_RESULT to return non-empty result
//         PROCESSOR_RESULT.with_borrow_mut(|r| *r = non_empty_result.clone());
//         service.receive_announce_to_compute(announce);

//         assert_eq!(service.next().await.unwrap().announce_hash, announce_hash);

//         // Verify block was marked as computed
//         assert!(db.announce_meta(announce_hash).computed);

//         // Verify transitions were stored in DB
//         let stored_transitions = db.announce_outcome(announce_hash).unwrap();
//         assert_eq!(stored_transitions.len(), 1);
//         assert_eq!(stored_transitions[0].actor_id, ActorId::from([1; 32]));
//         assert_eq!(stored_transitions[0].new_state_hash, H256::from([2; 32]));

//         // Verify latest announce
//         assert_eq!(
//             db.latest_data().unwrap().computed_announce_hash,
//             announce_hash
//         );
//     }
// }
