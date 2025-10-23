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

use crate::{ComputeError, ProcessorExt, Result, utils};
use ethexe_common::{
    Announce,  SimpleBlockData, HashOf,
    db::{
        AnnounceStorageRead, AnnounceStorageWrite, BlockMetaStorageRead, BlockMetaStorageWrite,
        CodesStorageRead, LatestDataStorageRead, LatestDataStorageWrite, OnChainStorageRead,
    },
    events::{BlockEvent, BlockRequestEvent, RouterEvent},
};
use ethexe_db::Database;
use ethexe_processor::BlockProcessingResult;
use gprimitives::{CodeId, H256};
use std::collections::HashSet;

#[derive(Default, Debug)]
pub(crate) struct MissingData {
    pub codes: HashSet<CodeId>,
    pub validated_codes: HashSet<CodeId>,
}

pub(crate) fn missing_data(db: &Database, block_hash: H256) -> Result<MissingData> {
    if !db.block_synced(block_hash) {
        return Err(ComputeError::BlockNotSynced(block_hash));
    }

    let chain = utils::collect_chain(db, block_hash, |meta| !meta.prepared)?;

    let mut missing_codes = HashSet::new();
    let mut missing_validated_codes = HashSet::new();

    for block in chain {
        let events = db
            .block_events(block.hash)
            .ok_or(ComputeError::BlockEventsNotFound(block.hash))?;

        for event in events {
            match event {
                BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, .. })
                    if db.code_valid(code_id).is_none() =>
                {
                    missing_codes.insert(code_id);
                }
                BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, .. })
                    if db.code_valid(code_id).is_none() =>
                {
                    missing_validated_codes.insert(code_id);
                    missing_codes.insert(code_id);
                }
                _ => {}
            }
        }
    }

    Ok(MissingData {
        codes: missing_codes,
        validated_codes: missing_validated_codes,
    })
}

pub(crate) async fn prepare(
    db: Database,
    mut processor: impl ProcessorExt,
    block_hash: H256,
) -> Result<()> {
    debug_assert!(
        db.block_synced(block_hash),
        "Block {block_hash} must be synced, checked in missing data"
    );
    debug_assert!(
        missing_data(&db, block_hash)
            .expect("Cannot collect missing data")
            .validated_codes
            .is_empty(),
        "Missing validated codes have to be loaded before prepare"
    );

    let chain = utils::collect_chain(&db, block_hash, |meta| !meta.prepared)?;
    for block in chain {
        prepare_one_block(&db, &mut processor, block).await?;
    }

    Ok(())
}

async fn prepare_one_block(
    db: &Database,
    processor: &mut impl ProcessorExt,
    block: SimpleBlockData,
) -> Result<()> {
    let parent = block.header.parent_hash;
    let mut requested_codes = HashSet::new();
    let mut validated_codes = HashSet::new();

    let parent_meta = db.block_meta(parent);
    let mut last_committed_batch = parent_meta
        .last_committed_batch
        .ok_or(ComputeError::LastCommittedBatchNotFound(parent))?;
    let mut codes_queue = parent_meta
        .codes_queue
        .ok_or(ComputeError::CodesQueueNotFound(parent))?;

    let mut last_committed_announce_hash = None;

    let events = db
        .block_events(block.hash)
        .ok_or(ComputeError::BlockEventsNotFound(block.hash))?;
    for event in events {
        match event {
            BlockEvent::Router(RouterEvent::BatchCommitted { digest }) => {
                last_committed_batch = digest;
            }
            BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, .. }) => {
                requested_codes.insert(code_id);
            }
            BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, .. }) => {
                validated_codes.insert(code_id);
            }
            BlockEvent::Router(RouterEvent::AnnouncesCommitted(head)) => {
                last_committed_announce_hash = Some(head);
            }
            _ => {}
        }
    }

    let parent_announces = parent_meta
        .announces
        .ok_or(ComputeError::PreparedBlockAnnouncesSetMissing(parent))?;
    if parent_announces.len() != 1 {
        todo!("TODO #4813: Currently supporting exactly one announce per block only");
    }
    let parent_announce_hash = parent_announces.first().copied().unwrap();

    let new_base_announce_hash = propagate_from_parent_announce(
        db,
        processor,
        block.hash,
        parent_announce_hash,
        last_committed_announce_hash,
    )
    .await?;

    codes_queue.retain(|code_id| !validated_codes.contains(code_id));
    codes_queue.extend(requested_codes);

    let last_committed_announce_hash = if let Some(hash) = last_committed_announce_hash {
        hash
    } else {
        parent_meta
            .last_committed_announce
            .ok_or(ComputeError::LastCommittedHeadNotFound(parent))?
    };

    db.mutate_block_meta(block.hash, |meta| {
        meta.last_committed_batch = Some(last_committed_batch);
        meta.codes_queue = Some(codes_queue);
        meta.announces = Some([new_base_announce_hash].into());
        meta.last_committed_announce = Some(last_committed_announce_hash);
        meta.prepared = true;
    });

    db.mutate_latest_data(|data| {
        data.prepared_block_hash = block.hash;
        data.computed_announce_hash = new_base_announce_hash;
    })
    .ok_or(ComputeError::LatestDataNotFound)?;

    Ok(())
}

/// Create a new base announce from provided parent announce hash.
/// Compute the announce and store related data in the database.
async fn propagate_from_parent_announce(
    db: &Database,
    processor: &mut impl ProcessorExt,
    block_hash: H256,
    parent_announce_hash: HashOf<Announce>,
    last_committed_announce_hash: Option<HashOf<Announce>>,
) -> Result<HashOf<Announce>> {
    if let Some(last_committed_announce_hash) = last_committed_announce_hash {
        log::trace!(
            "Searching for last committed announce hash {last_committed_announce_hash} in known announces chain",
        );

        let begin_announce_hash = db
            .latest_data()
            .ok_or(ComputeError::LatestDataNotFound)?
            .start_announce_hash;

        // TODO #4813: 1000 - temporary limit to determine last committed announce hash is from known chain
        // after we append announces mortality, we can remove this limit
        let mut announce_hash = parent_announce_hash;
        for _ in 0..1000 {
            if announce_hash == begin_announce_hash || announce_hash == last_committed_announce_hash
            {
                break;
            }

            announce_hash = db
                .announce(announce_hash)
                .ok_or(ComputeError::AnnounceNotFound(announce_hash))?
                .parent;
        }

        // TODO #4813: temporary check, remove after announces mortality is implemented
        assert_eq!(
            announce_hash, last_committed_announce_hash,
            "Cannot find last committed announce hash in known announces chain"
        );
    }

    // TODO #4814: hack - use here base with gas to avoid unknown announces in tests,
    // this can be fixed by unknown announces handling later
    let new_base_announce = Announce::with_default_gas(block_hash, parent_announce_hash);
    let new_base_announce_hash = new_base_announce.to_hash();

    if db.announce_meta(new_base_announce_hash).computed {
        // One possible case is:
        // node execution was dropped before block was marked as prepared,
        // but announce was already marked as computed.
        // see also `announce_is_computed_and_included`
        log::warn!(
            "Announce {new_base_announce_hash:?} was already computed,
             means it was lost by some reasons, skip computation,
             but setting it as announce in block {block_hash:?}"
        );

        return Ok(new_base_announce_hash);
    }

    let events = db
        .block_events(block_hash)
        .ok_or(ComputeError::BlockEventsNotFound(block_hash))?
        .into_iter()
        .filter_map(|event| event.to_request())
        .collect::<Vec<BlockRequestEvent>>();

    let BlockProcessingResult {
        transitions,
        states,
        schedule,
    } = processor
        .process_announce(new_base_announce.clone(), events)
        .await?;

    db.set_announce(new_base_announce);
    db.set_announce_outcome(new_base_announce_hash, transitions);
    db.set_announce_program_states(new_base_announce_hash, states);
    db.set_announce_schedule(new_base_announce_hash, schedule);
    db.mutate_announce_meta(new_base_announce_hash, |meta| {
        meta.computed = true;
    });

    Ok(new_base_announce_hash)
}

#[cfg(test)]
mod tests {
    use crate::tests::MockProcessor;

    use super::*;
    use ethexe_common::{Address, BlockHeader, Digest, db::*, events::BlockEvent, HashOf};
    use ethexe_db::Database as DB;
    use gprimitives::H256;
    use nonempty::nonempty;

    #[tokio::test]
    async fn test_propagate_data_from_parent() {
        let db = DB::memory();
        let block_hash = H256::random();
        let parent_announce_hash = HashOf::random();

        db.set_block_events(block_hash, &[]);

        let announce_hash = propagate_from_parent_announce(
            &db,
            &mut MockProcessor,
            block_hash,
            parent_announce_hash,
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            db.announce(announce_hash).unwrap(),
            Announce::with_default_gas(block_hash, parent_announce_hash),
            "incorrect announce was stored"
        );
        assert_eq!(db.announce_outcome(announce_hash), Some(Default::default()));
        assert_eq!(
            db.announce_schedule(announce_hash),
            Some(Default::default())
        );
        assert_eq!(
            db.announce_program_states(announce_hash),
            Some(Default::default())
        );
        assert!(db.announce_meta(announce_hash).computed);
    }

    #[tokio::test]
    async fn test_prepare_one_block() {
        let db = DB::memory();
        let parent_hash = H256::random();
        let block = SimpleBlockData {
            hash: H256::random(),
            header: BlockHeader {
                height: 1,
                timestamp: 1000,
                parent_hash,
            },
        };
        let last_committed_announce = HashOf::random();
        let code1_id = CodeId::from([1u8; 32]);
        let code2_id = CodeId::from([2u8; 32]);
        let batch_committed = Digest::random();
        let validators = nonempty![Address::from([42u8; 20])];

        let parent_announce = Announce::base(parent_hash, last_committed_announce);
        db.set_announce(parent_announce.clone());
        db.set_announce_outcome(parent_announce.to_hash(), Default::default());
        db.set_announce_schedule(parent_announce.to_hash(), Default::default());
        db.set_announce_program_states(parent_announce.to_hash(), Default::default());

        db.mutate_block_meta(parent_hash, |meta| {
            *meta = BlockMeta {
                prepared: true,
                announces: Some([parent_announce.to_hash()].into()),
                codes_queue: Some(vec![code1_id].into()),
                last_committed_batch: Some(Digest::random()),
                last_committed_announce: Some(HashOf::random()),
            }
        });
        db.set_block_validators(parent_hash, validators.clone());

        db.set_block_header(block.hash, block.header);

        db.set_latest_data(Default::default());

        let events = vec![
            BlockEvent::Router(RouterEvent::BatchCommitted {
                digest: batch_committed,
            }),
            BlockEvent::Router(RouterEvent::AnnouncesCommitted(parent_announce.to_hash())),
            BlockEvent::Router(RouterEvent::CodeGotValidated {
                code_id: code1_id,
                valid: true,
            }),
            BlockEvent::Router(RouterEvent::CodeValidationRequested {
                code_id: code2_id,
                timestamp: 1000,
                tx_hash: H256::random(),
            }),
        ];
        db.set_block_events(block.hash, &events);
        db.set_block_validators(block.hash, validators);
        db.set_block_synced(block.hash);

        // Prepare the block
        prepare_one_block(&db, &mut MockProcessor, block.clone())
            .await
            .unwrap();

        let meta = db.block_meta(block.hash);
        assert!(meta.prepared);
        assert_eq!(meta.codes_queue, Some(vec![code2_id].into()),);
        assert_eq!(meta.last_committed_batch, Some(batch_committed),);
        assert_eq!(
            meta.last_committed_announce,
            Some(parent_announce.to_hash())
        );
        assert_eq!(meta.announces.as_ref().map(|a| a.len()), Some(1));

        let announce_hash = meta.announces.unwrap().first().copied().unwrap();
        let announce = db.announce(announce_hash).unwrap();
        assert_eq!(
            announce,
            Announce::with_default_gas(block.hash, parent_announce.to_hash())
        );
        assert!(db.announce_meta(announce_hash).computed);
        assert_eq!(db.announce_outcome(announce_hash), Some(Default::default()));
        assert_eq!(
            db.announce_schedule(announce_hash),
            Some(Default::default())
        );
        assert_eq!(
            db.announce_program_states(announce_hash),
            Some(Default::default())
        );
        assert_eq!(db.latest_data().unwrap().prepared_block_hash, block.hash);
    }
}
