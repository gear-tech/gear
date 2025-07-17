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

use crate::{compute, utils, ComputeError, ProcessorExt, Result};
use ethexe_common::{
    db::{
        AnnounceStorageRead, AnnounceStorageWrite, BlockMetaStorageRead, BlockMetaStorageWrite,
        CodesStorageRead, OnChainStorageRead,
    },
    events::{BlockEvent, RouterEvent},
    AnnounceHash, ProducerBlock, SimpleBlockData,
};
use ethexe_db::Database;
use ethexe_processor::BlockProcessingResult;
use gprimitives::{CodeId, H256};
use std::collections::{HashSet, VecDeque};

#[derive(Default)]
pub(crate) struct MissingData {
    pub codes: HashSet<CodeId>,
    pub validated_codes: HashSet<CodeId>,
    pub announces_request: Option<(AnnounceHash, u32)>,
}

pub(crate) fn missing_data(
    db: &Database,
    block_hash: H256,
    commitment_delay_limit: u32,
) -> Result<MissingData> {
    let chain = utils::collect_chain(db, block_hash, |meta| !meta.prepared)?;

    let Some(first_not_prepared_block_height) = chain.front().map(|block| block.header.height)
    else {
        return Ok(MissingData::default());
    };

    let mut missing_codes = HashSet::new();
    let mut missing_validated_codes = HashSet::new();
    let mut last_committed_unknown = None;

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
                BlockEvent::Router(RouterEvent::GearBlockCommitted(announce))
                    if !announce.is_base() && db.announce(announce.hash()).is_none() =>
                {
                    last_committed_unknown = Some(announce);
                }
                _ => {}
            }
        }
    }

    let announces_request = last_committed_unknown
        .map(|announce| -> Result<(AnnounceHash, u32)> {
            let corresponding_block_height = db
                .block_header(announce.hash)
                .ok_or(ComputeError::BlockHeaderNotFound(announce.hash))?
                .height;
            let request_len = corresponding_block_height
                .checked_sub(first_not_prepared_block_height.saturating_sub(commitment_delay_limit))
                .expect(
                    "TODO +_+_+: announce committed too far from corresponding block - currently not supported",
                );
            Ok((announce.hash(), request_len))
        })
        .transpose()?;

    Ok(MissingData {
        codes: missing_codes,
        validated_codes: missing_validated_codes,
        announces_request,
    })
}

pub(crate) async fn prepare(
    db: Database,
    mut processor: impl ProcessorExt,
    block_hash: H256,
    commitment_delay_limit: u32,
) -> Result<()> {
    // +_+_+ debug assert that all data is loaded

    let chain = utils::collect_chain(&db, block_hash, |meta| !meta.prepared)?;
    for block in chain {
        propagate_from_parent_block(&db, &mut processor, block, commitment_delay_limit).await?;
    }

    Ok(())
}

// TODO +_+_+: Implement validation logic
#[allow(unused)]
pub(crate) fn validate(_db: &Database, _block_hash: H256) -> Result<()> {
    Ok(())
}

async fn propagate_from_parent_block(
    db: &Database,
    processor: &mut impl ProcessorExt,
    block: SimpleBlockData,
    commitment_delay_limit: u32,
) -> Result<()> {
    let parent = block.header.parent_hash;
    let mut requested_codes = HashSet::new();
    let mut validated_codes = HashSet::new();

    let parent_meta = db.block_meta(parent);
    let mut last_committed_batch = parent_meta
        .last_committed_batch
        .ok_or_else(|| ComputeError::LastCommittedBatchNotFound(parent))?;
    let mut codes_queue = parent_meta
        .codes_queue
        .ok_or(ComputeError::CodesQueueNotFound(parent))?;

    // last committed announce hash
    let mut head_announce_hash = None;

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
            BlockEvent::Router(RouterEvent::GearBlockCommitted(announce)) => {
                head_announce_hash = Some(announce.hash());
            }
            _ => {}
        }
    }

    let last_committed_announce_hash = if let Some(head_announce_hash) = head_announce_hash {
        let head_announce = db
            .announce(head_announce_hash)
            .expect("Announce must be loaded before block prepare");

        let head_announce_height = db
            .block_header(head_announce.block_hash)
            .expect("Block header must be loaded before block prepare")
            .height;
        assert!(
            head_announce_height < block.header.height,
            "Any committed announce must be from previous blocks"
        );

        if block.header.height - head_announce_height > commitment_delay_limit {
            assert!(
                head_announce.is_base(),
                "Head announce must be base announce"
            );
            assert!(
                db.announce_meta(head_announce_hash).computed,
                "Head announce must be already computed"
            );
            assert!({
                let mut is_predecessor: bool = true;
                for announce in db
                    .block_meta(parent)
                    .announces
                    .expect("Announces must be set for parent block")
                {
                    let mut predecessor = announce;
                    for _ in head_announce_height..block.header.height {
                        predecessor = db
                            .announce(predecessor)
                            .expect("Announce must be loaded before block prepare")
                            .parent;
                    }
                    if predecessor != head_announce_hash {
                        is_predecessor = false;
                        break;
                    }
                }

                is_predecessor
            });

            None
        } else {
            // in that case head announce can be not computed by this node

            let mut not_computed_chain = VecDeque::new();
            let mut announce_hash = head_announce_hash;
            let mut counter = 0;
            while !db.announce_meta(announce_hash).computed {
                counter += 1;
                assert!(
                    counter <= commitment_delay_limit,
                    "Chain of announces must not be longer than commitment delay limit"
                );

                let announce = db
                    .announce(announce_hash)
                    .expect("Announce must be loaded before block prepare");
                announce_hash = announce.parent;
                not_computed_chain.push_front(announce);
            }

            for announce in not_computed_chain {
                compute::compute(db.clone(), processor.clone(), announce).await?;
            }

            Some(head_announce_hash)
        }
    } else {
        None
    };

    let latest_announce_hash = last_committed_announce_hash.unwrap_or_default();

    // Propagate new base announces from all parent announces
    let parent_announces = parent_meta.announces.expect("Parent announces must be set");
    assert!(
        !parent_announces.is_empty(),
        "Parent block must have at least one announce"
    );
    let mut new_base_announce_hashes = Vec::new();
    for parent_announce_hash in parent_announces {
        if let Some(new_announce_hash) = propagate_from_parent_announce(
            db,
            processor,
            block.hash,
            parent_announce_hash,
            latest_announce_hash,
            commitment_delay_limit,
        )? {
            new_base_announce_hashes.push(new_announce_hash);
        }
    }
    assert!(
        !new_base_announce_hashes.is_empty(),
        "At least one announce must be propagated from parent block"
    );

    codes_queue.retain(|code_id| !validated_codes.contains(code_id));
    codes_queue.extend(requested_codes);

    db.mutate_block_meta(block.hash, |meta| {
        meta.last_committed_batch = Some(last_committed_batch);
        meta.codes_queue = Some(codes_queue);
        meta.announces = Some(new_base_announce_hashes);
        meta.prepared = true;
    });

    Ok(())
}

fn propagate_from_parent_announce<
    'a,
    DB: BlockMetaStorageRead + BlockMetaStorageWrite + AnnounceStorageRead + AnnounceStorageWrite,
>(
    db: &DB,
    processor: &mut impl ProcessorExt,
    block_hash: H256,
    parent: AnnounceHash,
    latest_announce_hash: AnnounceHash,
    commitment_delay_limit: u32,
) -> Result<Option<AnnounceHash>> {
    let mut predecessor = parent;
    for i in 0..commitment_delay_limit {
        if predecessor == latest_announce_hash {
            break;
        }

        let predecessor_announce = db
            .announce(predecessor)
            .ok_or_else(|| ComputeError::AnnounceNotFound(predecessor))?;

        if i == commitment_delay_limit - 1 && !predecessor_announce.is_base() {
            // We reached the oldest announce in commitment delay limit and which is not not committed.
            // This announce cannot be committed any more,
            // so if it is not base announce, we have to skip propagation from `parent`.
            return Ok(None);
        }

        predecessor = predecessor_announce.parent;
    }

    let new_base_announce = ProducerBlock::base(block_hash, parent);
    let new_base_announce_hash = new_base_announce.hash();

    let BlockProcessingResult {
        transitions,
        states,
        schedule,
    } = processor.process_base_announce(new_base_announce.clone())?;

    db.set_announce(new_base_announce);
    db.set_announce_outcome(new_base_announce_hash, transitions);
    db.set_announce_program_states(new_base_announce_hash, states);
    db.set_announce_schedule(new_base_announce_hash, schedule);
    db.mutate_announce_meta(new_base_announce_hash, |meta| {
        meta.computed = true;
    });

    Ok(Some(new_base_announce_hash))
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use ethexe_common::{
//         db::{BlockMetaStorageWrite, CodesStorageWrite, OnChainStorageWrite},
//         events::BlockEvent,
//         BlockHeader, Digest,
//     };
//     use ethexe_db::Database as DB;
//     use gprimitives::{CodeId, H256};
//     use std::collections::VecDeque;

//     /// Tests propagate_data_from_parent with empty events list
//     #[test]
//     fn test_propagate_data_from_parent_empty_events() {
//         let db = DB::memory();
//         let block_hash = H256::from([2; 32]);
//         let parent_hash = H256::from([1; 32]);

//         // Set initial data for parent block
//         let initial_digest = Digest([42; 32]);
//         db.set_last_committed_batch(parent_hash, initial_digest);
//         db.set_block_codes_queue(parent_hash, VecDeque::new());

//         let events = Vec::<BlockEvent>::new();

//         let result =
//             propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

//         assert!(result.0.is_empty()); // missing_codes
//         assert!(result.1.is_empty()); // missing_validated_codes

//         // Verify that data was propagated from parent
//         let expected_digest = Digest([42; 32]);
//         assert_eq!(db.last_committed_batch(block_hash), Some(expected_digest));
//         assert_eq!(db.block_codes_queue(block_hash), Some(VecDeque::new()));
//     }

//     /// Tests propagate_data_from_parent with BatchCommitted event
//     #[test]
//     fn test_propagate_data_from_parent_batch_committed() {
//         let db = DB::memory();
//         let block_hash = H256::from([2; 32]);
//         let parent_hash = H256::from([1; 32]);

//         // Set initial data for parent block
//         let initial_digest = Digest([42; 32]);
//         db.set_last_committed_batch(parent_hash, initial_digest);
//         db.set_block_codes_queue(parent_hash, VecDeque::new());

//         let new_digest = Digest([99; 32]);
//         let events = [BlockEvent::Router(
//             ethexe_common::events::RouterEvent::BatchCommitted { digest: new_digest },
//         )];

//         let result =
//             propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

//         assert!(result.0.is_empty());
//         assert!(result.1.is_empty());

//         // Verify that last_committed_batch was updated
//         assert_eq!(db.last_committed_batch(block_hash), Some(new_digest));
//     }

//     /// Tests propagate_data_from_parent with CodeValidationRequested for existing code
//     #[test]
//     fn test_propagate_data_from_parent_code_validation_requested_existing() {
//         let db = DB::memory();
//         let block_hash = H256::from([2; 32]);
//         let parent_hash = H256::from([1; 32]);
//         let code_id = CodeId::from([3; 32]);

//         // Set initial data for parent block
//         db.set_last_committed_batch(parent_hash, Digest([42; 32]));
//         db.set_block_codes_queue(parent_hash, VecDeque::new());

//         // Add code to DB as valid
//         db.set_code_valid(code_id, true);

//         let events = [BlockEvent::Router(
//             ethexe_common::events::RouterEvent::CodeValidationRequested {
//                 code_id,
//                 timestamp: 1000,
//                 tx_hash: H256::from([4; 32]),
//             },
//         )];

//         let result =
//             propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

//         assert!(result.0.is_empty()); // missing_codes - code exists in DB
//         assert!(result.1.is_empty()); // missing_validated_codes

//         // Verify that code was added to queue
//         let codes_queue = db.block_codes_queue(block_hash).unwrap();
//         assert!(codes_queue.contains(&code_id));
//     }

//     /// Tests propagate_data_from_parent with CodeValidationRequested for missing code
//     #[test]
//     fn test_propagate_data_from_parent_code_validation_requested_missing() {
//         let db = DB::memory();
//         let block_hash = H256::from([2; 32]);
//         let parent_hash = H256::from([1; 32]);
//         let code_id = CodeId::from([3; 32]);

//         // Set initial data for parent block
//         db.set_last_committed_batch(parent_hash, Digest([42; 32]));
//         db.set_block_codes_queue(parent_hash, VecDeque::new());

//         let events = [BlockEvent::Router(
//             ethexe_common::events::RouterEvent::CodeValidationRequested {
//                 code_id,
//                 timestamp: 1000,
//                 tx_hash: H256::from([4; 32]),
//             },
//         )];

//         let result =
//             propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

//         assert!(result.0.contains(&code_id)); // missing_codes
//         assert!(result.1.is_empty()); // missing_validated_codes

//         // Verify that code was added to queue
//         let codes_queue = db.block_codes_queue(block_hash).unwrap();
//         assert!(codes_queue.contains(&code_id));
//     }

//     /// Tests propagate_data_from_parent with CodeGotValidated for missing code
//     #[test]
//     fn test_propagate_data_from_parent_code_got_validated_missing() {
//         let db = DB::memory();
//         let block_hash = H256::from([2; 32]);
//         let parent_hash = H256::from([1; 32]);
//         let code_id = CodeId::from([3; 32]);

//         // Set initial data for parent block
//         db.set_last_committed_batch(parent_hash, Digest([42; 32]));
//         db.set_block_codes_queue(parent_hash, VecDeque::new());

//         let events = [BlockEvent::Router(
//             ethexe_common::events::RouterEvent::CodeGotValidated {
//                 code_id,
//                 valid: true,
//             },
//         )];

//         let result =
//             propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

//         assert!(result.0.contains(&code_id)); // missing_codes
//         assert!(result.1.contains(&code_id)); // missing_validated_codes
//     }

//     /// Tests propagate_data_from_parent with CodeGotValidated for existing code with matching status
//     #[test]
//     fn test_propagate_data_from_parent_code_got_validated_matching_status() {
//         let db = DB::memory();
//         let block_hash = H256::from([2; 32]);
//         let parent_hash = H256::from([1; 32]);
//         let code_id = CodeId::from([3; 32]);

//         // Set initial data for parent block
//         db.set_last_committed_batch(parent_hash, Digest([42; 32]));
//         db.set_block_codes_queue(parent_hash, VecDeque::new());

//         // Add code to DB as valid
//         db.set_code_valid(code_id, true);

//         let events = [BlockEvent::Router(
//             ethexe_common::events::RouterEvent::CodeGotValidated {
//                 code_id,
//                 valid: true,
//             },
//         )];

//         let result =
//             propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

//         assert!(result.0.is_empty()); // missing_codes
//         assert!(result.1.is_empty()); // missing_validated_codes
//     }

//     /// Tests propagate_data_from_parent with CodeGotValidated for existing code with mismatched status
//     #[test]
//     fn test_propagate_data_from_parent_code_got_validated_mismatched_status() {
//         let db = DB::memory();
//         let block_hash = H256::from([2; 32]);
//         let parent_hash = H256::from([1; 32]);
//         let code_id = CodeId::from([3; 32]);

//         // Set initial data for parent block
//         db.set_last_committed_batch(parent_hash, Digest([42; 32]));
//         db.set_block_codes_queue(parent_hash, VecDeque::new());

//         // Add code to DB as valid
//         db.set_code_valid(code_id, true);

//         let events = [BlockEvent::Router(
//             ethexe_common::events::RouterEvent::CodeGotValidated {
//                 code_id,
//                 valid: false, // mismatched status
//             },
//         )];

//         let result = propagate_data_from_parent(&db, block_hash, parent_hash, events.iter());

//         // Should return CodeValidationStatusMismatch error
//         assert!(matches!(
//             result,
//             Err(ComputeError::CodeValidationStatusMismatch {
//                 code_id: err_code_id,
//                 local_status: true,
//                 remote_status: false,
//             }) if err_code_id == code_id
//         ));
//     }

//     /// Tests propagate_data_from_parent with other events (which are ignored)
//     #[test]
//     fn test_propagate_data_from_parent_other_events() {
//         let db = DB::memory();
//         let block_hash = H256::from([2; 32]);
//         let parent_hash = H256::from([1; 32]);

//         // Set initial data for parent block
//         db.set_last_committed_batch(parent_hash, Digest([42; 32]));
//         db.set_block_codes_queue(parent_hash, VecDeque::new());

//         let events = vec![
//             BlockEvent::Router(
//                 ethexe_common::events::RouterEvent::ComputationSettingsChanged {
//                     threshold: 100,
//                     wvara_per_second: 200,
//                 },
//             ),
//             BlockEvent::Router(ethexe_common::events::RouterEvent::ProgramCreated {
//                 actor_id: gprimitives::ActorId::from([5; 32]),
//                 code_id: CodeId::from([6; 32]),
//             }),
//         ];

//         let result =
//             propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

//         assert!(result.0.is_empty()); // missing_codes
//         assert!(result.1.is_empty()); // missing_validated_codes
//     }

//     /// Tests propagate_data_from_parent with combination of events
//     #[test]
//     fn test_propagate_data_from_parent_combined_events() {
//         let db = DB::memory();
//         let block_hash = H256::from([2; 32]);
//         let parent_hash = H256::from([1; 32]);
//         let code_id1 = CodeId::from([3; 32]);
//         let code_id2 = CodeId::from([4; 32]);
//         let code_id3 = CodeId::from([5; 32]);

//         // Set initial data for parent block
//         db.set_last_committed_batch(parent_hash, Digest([42; 32]));
//         db.set_block_codes_queue(parent_hash, VecDeque::new());

//         // Code2 already exists in DB
//         db.set_code_valid(code_id2, true);

//         let new_digest = Digest([99; 32]);
//         let events = vec![
//             BlockEvent::Router(ethexe_common::events::RouterEvent::BatchCommitted {
//                 digest: new_digest,
//             }),
//             BlockEvent::Router(
//                 ethexe_common::events::RouterEvent::CodeValidationRequested {
//                     code_id: code_id1,
//                     timestamp: 1000,
//                     tx_hash: H256::from([7; 32]),
//                 },
//             ),
//             BlockEvent::Router(
//                 ethexe_common::events::RouterEvent::CodeValidationRequested {
//                     code_id: code_id2,
//                     timestamp: 1001,
//                     tx_hash: H256::from([8; 32]),
//                 },
//             ),
//             BlockEvent::Router(ethexe_common::events::RouterEvent::CodeGotValidated {
//                 code_id: code_id3,
//                 valid: true,
//             }),
//         ];

//         let result =
//             propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

//         // code_id1 - missing (requested but not found)
//         // code_id3 - missing and validated (got validation but not found)
//         assert_eq!(result.0.len(), 2); // missing_codes: code_id1, code_id3
//         assert!(result.0.contains(&code_id1));
//         assert!(result.0.contains(&code_id3));
//         assert_eq!(result.1.len(), 1); // missing_validated_codes: code_id3
//         assert!(result.1.contains(&code_id3));

//         // Verify updates
//         assert_eq!(db.last_committed_batch(block_hash), Some(new_digest));

//         let codes_queue = db.block_codes_queue(block_hash).unwrap();
//         assert!(codes_queue.contains(&code_id1));
//         assert!(codes_queue.contains(&code_id2));
//         assert!(!codes_queue.contains(&code_id3)); // this code was removed from queue
//     }

//     /// Tests prepare with empty chain of blocks
//     #[test]
//     fn test_prepare_empty_chain() {
//         let db = DB::memory();
//         let head = H256::from([10; 32]);

//         // Create block as already prepared
//         db.mutate_block_meta(head, |m| {
//             m.synced = true;
//             m.prepared = true; // block is already prepared
//         });

//         let result = prepare(&db, head).unwrap();

//         assert!(result.chain.is_empty());
//         assert!(result.missing_codes.is_empty());
//         assert!(result.missing_validated_codes.is_empty());
//     }

//     /// Tests prepare with single block without events
//     #[test]
//     fn test_prepare_single_block_no_events() {
//         let db = DB::memory();
//         let parent_hash = H256::from([1; 32]);
//         let head = H256::from([10; 32]);

//         // Set initial data for parent block (required for propagate_data_from_parent)
//         db.set_last_committed_batch(parent_hash, Digest([42; 32]));
//         db.set_block_codes_queue(parent_hash, VecDeque::new());

//         // Configure parent as prepared
//         db.mutate_block_meta(parent_hash, |m| {
//             m.synced = true;
//             m.prepared = true;
//         });

//         // Configure head as unprepared
//         db.mutate_block_meta(head, |m| m.synced = true);

//         let header = BlockHeader {
//             height: 1,
//             parent_hash,
//             timestamp: 2000,
//         };
//         db.set_block_header(head, header.clone());

//         // Empty events
//         db.set_block_events(head, &[]);

//         let result = prepare(&db, head).unwrap();

//         assert_eq!(result.chain.len(), 1);
//         assert_eq!(result.chain[0].hash, head);
//         assert_eq!(result.chain[0].header, header);
//         assert!(result.missing_codes.is_empty());
//         assert!(result.missing_validated_codes.is_empty());
//     }

//     /// Tests prepare with single block with events
//     #[test]
//     fn test_prepare_single_block_with_events() {
//         let db = DB::memory();
//         let parent_hash = H256::from([1; 32]);
//         let head = H256::from([10; 32]);
//         let code_id = CodeId::from([20; 32]);

//         // Set initial data for parent block (required for propagate_data_from_parent)
//         db.set_last_committed_batch(parent_hash, Digest([42; 32]));
//         db.set_block_codes_queue(parent_hash, VecDeque::new());

//         // Configure parent as prepared
//         db.mutate_block_meta(parent_hash, |m| {
//             m.synced = true;
//             m.prepared = true;
//         });

//         // Configure head as unprepared
//         db.mutate_block_meta(head, |m| m.synced = true);

//         let header = BlockHeader {
//             height: 2,
//             parent_hash,
//             timestamp: 2000,
//         };
//         db.set_block_header(head, header.clone());

//         // Events with code validation request
//         let events = vec![BlockEvent::Router(
//             ethexe_common::events::RouterEvent::CodeValidationRequested {
//                 code_id,
//                 timestamp: 1000,
//                 tx_hash: H256::from([30; 32]),
//             },
//         )];
//         db.set_block_events(head, &events);

//         let result = prepare(&db, head).unwrap();

//         assert_eq!(result.chain.len(), 1);
//         assert_eq!(result.chain[0].hash, head);
//         assert!(result.missing_codes.contains(&code_id));
//         assert!(result.missing_validated_codes.is_empty());
//     }

//     /// Tests prepare with multiple blocks
//     #[test]
//     fn test_prepare_multiple_blocks() {
//         let db = DB::memory();
//         let grandparent_hash = H256::from([0; 32]);
//         let parent_hash = H256::from([1; 32]);
//         let head = H256::from([10; 32]);
//         let code_id1 = CodeId::from([20; 32]);
//         let code_id2 = CodeId::from([21; 32]);

//         // Set initial data for grandparent block
//         db.set_last_committed_batch(grandparent_hash, Digest([42; 32]));
//         db.set_block_codes_queue(grandparent_hash, VecDeque::new());

//         // Configure grandparent as prepared
//         db.mutate_block_meta(grandparent_hash, |m| {
//             m.synced = true;
//             m.prepared = true;
//         });

//         // Configure parent as unprepared
//         db.mutate_block_meta(parent_hash, |m| m.synced = true);

//         // Configure head as unprepared
//         db.mutate_block_meta(head, |m| m.synced = true);

//         let parent_header = BlockHeader {
//             height: 1,
//             parent_hash: grandparent_hash,
//             timestamp: 1500,
//         };
//         db.set_block_header(parent_hash, parent_header.clone());

//         let head_header = BlockHeader {
//             height: 2,
//             parent_hash,
//             timestamp: 2000,
//         };
//         db.set_block_header(head, head_header.clone());

//         // Events for parent block
//         let parent_events = vec![BlockEvent::Router(
//             ethexe_common::events::RouterEvent::CodeValidationRequested {
//                 code_id: code_id1,
//                 timestamp: 1000,
//                 tx_hash: H256::from([30; 32]),
//             },
//         )];
//         db.set_block_events(parent_hash, &parent_events);

//         // Events for head block
//         let head_events = vec![BlockEvent::Router(
//             ethexe_common::events::RouterEvent::CodeGotValidated {
//                 code_id: code_id2,
//                 valid: true,
//             },
//         )];
//         db.set_block_events(head, &head_events);

//         let result = prepare(&db, head).unwrap();

//         assert_eq!(result.chain.len(), 2);
//         // Blocks should be sorted from old to new
//         assert_eq!(result.chain[0].hash, parent_hash);
//         assert_eq!(result.chain[1].hash, head);

//         // Check missing codes from both blocks
//         assert!(result.missing_codes.contains(&code_id1)); // from parent
//         assert!(result.missing_codes.contains(&code_id2)); // from head
//         assert!(result.missing_validated_codes.contains(&code_id2)); // only from head
//     }

//     /// Tests prepare when block events are not found in DB
//     #[test]
//     fn test_prepare_missing_block_events() {
//         let db = DB::memory();
//         let parent_hash = H256::from([1; 32]);
//         let head = H256::from([10; 32]);

//         // Configure parent as prepared
//         db.mutate_block_meta(parent_hash, |m| {
//             m.synced = true;
//             m.prepared = true;
//         });

//         // Configure head as unprepared
//         db.mutate_block_meta(head, |m| m.synced = true);

//         let header = BlockHeader {
//             height: 1,
//             parent_hash,
//             timestamp: 2000,
//         };
//         db.set_block_header(head, header);

//         // DO NOT set events for block

//         let result = prepare(&db, head);

//         // Should return BlockEventsNotFound error
//         assert!(matches!(
//             result,
//             Err(ComputeError::BlockEventsNotFound(block_hash)) if block_hash == head
//         ));
//     }

//     /// Tests prepare with error from propagate_data_from_parent
//     #[test]
//     fn test_prepare_propagation_error() {
//         let db = DB::memory();
//         let parent_hash = H256::from([1; 32]);
//         let head = H256::from([10; 32]);
//         let code_id = CodeId::from([20; 32]);

//         // Set initial data for parent block (required for propagate_data_from_parent)
//         db.set_last_committed_batch(parent_hash, Digest([42; 32]));
//         db.set_block_codes_queue(parent_hash, VecDeque::new());

//         // Configure parent as prepared
//         db.mutate_block_meta(parent_hash, |m| {
//             m.synced = true;
//             m.prepared = true;
//         });

//         // Configure head as unprepared
//         db.mutate_block_meta(head, |m| m.synced = true);

//         let header = BlockHeader {
//             height: 1,
//             parent_hash,
//             timestamp: 2000,
//         };
//         db.set_block_header(head, header);

//         // Add code to DB as valid
//         db.set_code_valid(code_id, true);

//         // Events with mismatched validation status
//         let events = [BlockEvent::Router(
//             ethexe_common::events::RouterEvent::CodeGotValidated {
//                 code_id,
//                 valid: false, // mismatched status
//             },
//         )];
//         db.set_block_events(head, &events);

//         let result = prepare(&db, head);

//         // Should return error from propagate_data_from_parent
//         assert!(matches!(
//             result,
//             Err(ComputeError::CodeValidationStatusMismatch { .. })
//         ));
//     }
// }
