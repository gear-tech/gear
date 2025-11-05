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
    Announce, HashOf,
    db::{
        AnnounceStorageRO, AnnounceStorageRW, BlockMetaStorageRO, InjectedStorageRW,
        LatestDataStorageRW, OnChainStorageRO,
    },
};
use ethexe_db::Database;
use ethexe_processor::BlockProcessingResult;
use futures::future::BoxFuture;
use std::{
    collections::VecDeque,
    task::{Context, Poll},
};

pub struct ComputeSubService<P: ProcessorExt> {
    db: Database,
    processor: P,

    input: VecDeque<Announce>,
    computation: Option<BoxFuture<'static, Result<HashOf<Announce>>>>,
}

impl<P: ProcessorExt> ComputeSubService<P> {
    pub fn new(db: Database, processor: P) -> Self {
        Self {
            db,
            processor,
            input: VecDeque::new(),
            computation: None,
        }
    }

    pub fn receive_announce_to_compute(&mut self, announce: Announce) {
        self.input.push_back(announce);
    }

    async fn compute(
        db: Database,
        mut processor: P,
        announce: Announce,
    ) -> Result<HashOf<Announce>> {
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

        if announces_chain.is_empty() {
            log::trace!("All announces are already computed");
            return Ok(announce_hash);
        }

        for (announce_hash, announce) in announces_chain {
            Self::compute_one(&db, &mut processor, announce_hash, announce).await?;
        }

        Ok(announce_hash)
    }

    async fn compute_one(
        db: &Database,
        processor: &mut P,
        announce_hash: HashOf<Announce>,
        announce: Announce,
    ) -> Result<HashOf<Announce>> {
        let block_hash = announce.block_hash;

        let events = db
            .block_events(block_hash)
            .ok_or(ComputeError::BlockEventsNotFound(block_hash))?
            .into_iter()
            .filter_map(|event| event.to_request())
            .collect();

        let processing_result = processor.process_announce(announce.clone(), events).await?;

        let BlockProcessingResult {
            transitions,
            states,
            schedule,
            promises,
        } = processing_result;

        db.set_announce_outcome(announce_hash, transitions);
        db.set_announce_program_states(announce_hash, states);
        db.set_announce_schedule(announce_hash, schedule);
        db.mutate_announce_meta(announce_hash, |meta| {
            meta.computed = true;
        });

        // Add promises for injected transactions.
        promises.into_iter().for_each(|promise| {
            db.mutate_injected_tx(promise.tx_hash, |tx_with_meta| {
                tx_with_meta.promise = Some(promise);
            });
        });

        db.mutate_latest_data(|data| {
            data.computed_announce_hash = announce_hash;
        })
        .ok_or(ComputeError::LatestDataNotFound)?;

        Ok(announce_hash)
    }
}

impl<P: ProcessorExt> SubService for ComputeSubService<P> {
    type Output = HashOf<Announce>;

    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Output>> {
        if self.computation.is_none()
            && let Some(announce) = self.input.pop_front()
        {
            self.computation = Some(Box::pin(Self::compute(
                self.db.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{MockProcessor, PROCESSOR_RESULT};
    use ethexe_common::{db::*, gear::StateTransition, mock::*};
    use gprimitives::{ActorId, H256};

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn test_compute() {
        gear_utils::init_default_logger();

        let db = Database::memory();
        let block_hash = BlockChain::mock(1).setup(&db).blocks[1].hash;
        let mut service = ComputeSubService::new(db.clone(), MockProcessor);

        let announce = Announce {
            block_hash,
            parent: db.latest_data().unwrap().genesis_announce_hash,
            gas_allowance: Some(100),
            injected_transactions: vec![],
        };
        let announce_hash = announce.to_hash();

        // Create non-empty processor result with transitions
        let non_empty_result = BlockProcessingResult {
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
        service.receive_announce_to_compute(announce);

        assert_eq!(service.next().await.unwrap(), announce_hash);

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
}
