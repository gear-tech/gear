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

use crate::{ComputeError, ProcessorExt, Result};
use ethexe_common::{
    Announce, BlockMetaStorageRead, BlockMetaStorageWrite,
    db::{AnnounceStorageRead, AnnounceStorageWrite, OnChainStorageRead},
};
use ethexe_db::Database;
use ethexe_processor::BlockProcessingResult;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum ComputationStatus {
    Rejected,
    Computed,
}

pub(crate) async fn compute<P: ProcessorExt>(
    db: Database,
    mut processor: P,
    announce: Announce,
) -> Result<ComputationStatus> {
    let announce_hash = announce.hash();
    let block_hash = announce.block_hash;

    if db.announce_meta(announce_hash).computed {
        log::warn!("{announce:?} is already computed");
        return Ok(ComputationStatus::Computed);
    }

    if !db.announce_meta(announce.parent).computed {
        log::warn!(
            "{announce:?} is from unknown branch: parent {} not computed",
            announce.parent
        );
        return Ok(ComputationStatus::Rejected);
    }

    if !db.block_meta(block_hash).prepared {
        return Err(ComputeError::BlockNotPrepared(block_hash));
    }

    debug_assert!(
        !announce.is_base(),
        "Announce cannot be base, else it must be already computed in prepare"
    );

    let events = db
        .block_events(block_hash)
        .ok_or(ComputeError::BlockEventsNotFound(block_hash))?;

    let block_request_events = events
        .into_iter()
        .filter_map(|event| event.to_request())
        .collect();

    let processing_result = processor
        .process_announce(announce.clone(), block_request_events)
        .await?;

    let BlockProcessingResult {
        transitions,
        states,
        schedule,
    } = processing_result;

    // replace previous announce from corresponding block
    let old_announce = db
        .block_meta(block_hash)
        .announces
        .ok_or_else(|| ComputeError::AnnouncesNotFound(block_hash))?
        .pop()
        .expect("TODO: temporary panic - number of announces in prepared block must be always 1");

    // TODO +_+_+: bug here - announce marked as not computed before block meta is updated,
    // this should be fixed by using database transactions, which are not yet implemented
    db.mutate_announce_meta(old_announce, |meta| meta.computed = false);

    db.set_announce(announce);
    db.set_announce_outcome(announce_hash, transitions);
    db.set_announce_program_states(announce_hash, states);
    db.set_announce_schedule(announce_hash, schedule);

    // TODO +_+_+: bug here - announce marked as computed before block meta is updated,
    // this should be fixed by using database transactions, which are not yet implemented
    db.mutate_announce_meta(announce_hash, |meta| {
        meta.computed = true;
    });

    db.mutate_block_meta(block_hash, |meta| {
        meta.announces = Some(vec![announce_hash]);
    });

    Ok(ComputationStatus::Computed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{MockProcessor, PROCESSOR_RESULT};
    use ethexe_common::{
        AnnounceHash, BlockHeader, BlockMeta, SimpleBlockData,
        db::{BlockMetaStorageWrite, OnChainStorageWrite},
        gear::StateTransition,
    };
    use ethexe_db::Database as DB;
    use gprimitives::{ActorId, H256};
    use nonempty::NonEmpty;

    #[tokio::test]
    async fn test_compute() {
        let db = DB::memory();

        let genesis_hash = H256::random();
        let block_hash = H256::random();

        ethexe_common::set_genesis_in_db(
            &db,
            SimpleBlockData {
                hash: genesis_hash,
                header: BlockHeader {
                    height: 0,
                    timestamp: 1000,
                    parent_hash: H256::random(),
                },
            },
            NonEmpty::from_vec(vec![Default::default()]).unwrap(),
        );

        // Setup block as prepared
        db.mutate_block_meta(block_hash, |meta| {
            *meta = BlockMeta {
                announces: Some(vec![AnnounceHash::random()]),
                ..BlockMeta::default_prepared()
            }
        });
        db.set_block_events(block_hash, &[]);

        let announce = Announce {
            block_hash: block_hash,
            parent: AnnounceHash::zero(),
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
        let status = compute(db.clone(), MockProcessor, announce).await.unwrap();
        assert_eq!(status, ComputationStatus::Computed);

        // Verify block was marked as computed
        assert!(db.announce_meta(announce_hash).computed);

        // Verify transitions were stored in DB
        let stored_transitions = db.announce_outcome(announce_hash).unwrap();
        assert_eq!(stored_transitions.len(), 1);
        assert_eq!(stored_transitions[0].actor_id, ActorId::from([1; 32]));
        assert_eq!(stored_transitions[0].new_state_hash, H256::from([2; 32]));

        // Try with unknown parent
        let announce = Announce {
            block_hash: block_hash,
            parent: AnnounceHash::random(),
            gas_allowance: Some(100),
            off_chain_transactions: vec![],
        };
        let status = compute(db.clone(), MockProcessor, announce).await.unwrap();
        assert_eq!(status, ComputationStatus::Rejected);
    }
}
