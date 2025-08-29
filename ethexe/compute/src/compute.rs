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

use crate::{
    ComputeError, ProcessorExt, Result,
    utils::{self, announce_is_included},
};
use ethexe_common::{
    Announce, AnnounceHash,
    db::{
        AnnounceStorageRead, AnnounceStorageWrite, BlockMetaStorageRead, LatestDataStorageWrite,
        OnChainStorageRead,
    },
};
use ethexe_db::Database;
use ethexe_processor::BlockProcessingResult;
use gprimitives::H256;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum ComputationStatus {
    Rejected(AnnounceHash),
    Computed(AnnounceHash),
}

pub(crate) async fn compute_and_include<P: ProcessorExt>(
    db: Database,
    mut processor: P,
    announce: Announce,
) -> Result<ComputationStatus> {
    let announce_hash = announce.hash();
    let block_hash = announce.block_hash;

    if !db.block_meta(block_hash).prepared {
        log::error!("Block {block_hash} is not prepared before announce is coming");
        return Err(ComputeError::BlockNotPrepared(block_hash));
    }

    if db.announce_meta(announce_hash).computed {
        log::warn!("{announce:?} is already computed");
        return Ok(ComputationStatus::Computed(announce_hash));
    }

    if !db.announce_meta(announce.parent).computed {
        log::warn!(
            "{announce:?} is from unknown branch: parent {}",
            announce.parent
        );
        return Ok(ComputationStatus::Rejected(announce_hash));
    }

    let result = match compute_one(&db, &mut processor, announce.clone()).await {
        Ok(res) => res,
        Err(err) => {
            log::error!("Failed to process announce {announce_hash}: {err}");
            return Ok(ComputationStatus::Rejected(announce_hash));
        }
    };

    // Order is important here. All computed announces must be included first, so
    // we include it in the block and db
    crate::utils::include_one(&db, announce)?;
    // we set the computation results and mark announce as computed
    set_computation_result(&db, announce_hash, result);

    db.mutate_latest_data(|data| {
        data.computed_announce_hash = announce_hash;
    })
    .ok_or(ComputeError::LatestDataNotFound)?;

    Ok(ComputationStatus::Computed(announce_hash))
}

pub async fn compute_block_announces<P: ProcessorExt>(
    db: Database,
    mut processor: P,
    block_hash: H256,
) -> Result<H256> {
    let meta = db.block_meta(block_hash);

    if !meta.prepared {
        return Err(ComputeError::BlockNotPrepared(block_hash));
    }

    for announce_hash in meta
        .announces
        .ok_or(ComputeError::AnnouncesNotFound(block_hash))?
    {
        compute_chain(db.clone(), &mut processor, announce_hash).await?;
    }

    Ok(block_hash)
}

async fn compute_chain<P: ProcessorExt>(
    db: Database,
    processor: &mut P,
    head_announce_hash: AnnounceHash,
) -> Result<()> {
    debug_assert!(
        announce_is_included(&db, head_announce_hash),
        "can be called over already included announces only"
    );

    for announce in utils::not_computed_chain(&db, head_announce_hash)? {
        let announce_hash = announce.hash();
        let result = compute_one(&db, processor, announce).await?;
        set_computation_result(&db, announce_hash, result);
    }

    Ok(())
}

async fn compute_one<P: ProcessorExt>(
    db: &Database,
    processor: &mut P,
    announce: Announce,
) -> Result<BlockProcessingResult> {
    let block_hash = announce.block_hash;

    let events = db
        .block_events(block_hash)
        .ok_or(ComputeError::BlockEventsNotFound(block_hash))?;

    let block_request_events = events
        .into_iter()
        .filter_map(|event| event.to_request())
        .collect();

    processor
        .process_announce(announce.clone(), block_request_events)
        .await
}

fn set_computation_result<DB: AnnounceStorageWrite>(
    db: &DB,
    announce_hash: AnnounceHash,
    result: BlockProcessingResult,
) {
    db.set_announce_outcome(announce_hash, result.transitions);
    db.set_announce_program_states(announce_hash, result.states);
    db.set_announce_schedule(announce_hash, result.schedule);
    db.mutate_announce_meta(announce_hash, |meta| {
        meta.computed = true;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{MockProcessor, PROCESSOR_RESULT};
    use ethexe_common::{
        AnnounceHash, BlockHeader, SimpleBlockData, db::*, gear::StateTransition, mock::*,
    };
    use ethexe_db::Database as DB;
    use gprimitives::{ActorId, H256};
    use nonempty::NonEmpty;

    #[tokio::test]
    async fn test_compute() {
        gear_utils::init_default_logger();

        let db = DB::memory();
        let block_hash = BlockChain::mock(1).setup(&db).blocks[1].hash;

        let announce = Announce {
            block_hash,
            parent: db.latest_data().unwrap().genesis_announce_hash,
            gas_allowance: Some(100),
            off_chain_transactions: vec![],
        };
        let announce_hash = announce.hash();

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
        PROCESSOR_RESULT.with(|r| *r.borrow_mut() = non_empty_result.clone());
        let status = compute_and_include(db.clone(), MockProcessor, announce)
            .await
            .unwrap();
        assert_eq!(status, ComputationStatus::Computed(announce_hash));

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

        // Try with unknown parent
        let announce = Announce {
            block_hash,
            parent: AnnounceHash::random(),
            gas_allowance: Some(100),
            off_chain_transactions: vec![],
        };
        let announce_hash = announce.hash();
        let status = compute_and_include(db.clone(), MockProcessor, announce)
            .await
            .unwrap();
        assert_eq!(status, ComputationStatus::Rejected(announce_hash));
    }
}
