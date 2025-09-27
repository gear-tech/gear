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

use crate::{ComputeError, Result, utils};
use ethexe_common::{
    SimpleBlockData,
    db::{BlockMetaStorageRead, BlockMetaStorageWrite, CodesStorageRead, OnChainStorageRead},
    events::{BlockEvent, RouterEvent},
};
use gprimitives::{CodeId, H256};
use std::collections::{HashSet, VecDeque};

#[derive(Debug)]
pub(crate) struct PrepareInfo {
    pub chain: VecDeque<SimpleBlockData>,
    pub missing_codes: HashSet<CodeId>,
    pub missing_validated_codes: HashSet<CodeId>,
}

pub(crate) fn prepare<
    DB: OnChainStorageRead + BlockMetaStorageRead + BlockMetaStorageWrite + CodesStorageRead,
>(
    db: &DB,
    head: H256,
) -> Result<PrepareInfo> {
    let chain = utils::collect_chain(db, head, |meta| !meta.prepared)?;

    let mut missing_codes = HashSet::new();
    let mut missing_validated_codes = HashSet::new();

    for block in chain.iter() {
        let events = db
            .block_events(block.hash)
            .ok_or(ComputeError::BlockEventsNotFound(block.hash))?;
        let (block_missing_codes, block_missing_validated_codes) =
            propagate_data_from_parent(db, block.hash, block.header.parent_hash, events.iter())?;
        missing_codes.extend(block_missing_codes);
        missing_validated_codes.extend(block_missing_validated_codes);
    }

    Ok(PrepareInfo {
        chain,
        missing_codes,
        missing_validated_codes,
    })
}

/// # Return
/// (all missing codes, missing codes that have been already validated)
fn propagate_data_from_parent<
    'a,
    DB: BlockMetaStorageRead + BlockMetaStorageWrite + CodesStorageRead,
>(
    db: &DB,
    block: H256,
    parent: H256,
    events: impl Iterator<Item = &'a BlockEvent>,
) -> Result<(HashSet<CodeId>, HashSet<CodeId>)> {
    let parent_meta = db.block_meta(parent);

    let mut missing_codes = HashSet::new();
    let mut missing_validated_codes = HashSet::new();
    let mut requested_codes = HashSet::new();
    let mut validated_codes = HashSet::new();
    let mut last_committed_batch = parent_meta
        .last_committed_batch
        .ok_or(ComputeError::LastCommittedBatchNotFound(parent))?;
    let mut last_committed_head = parent_meta
        .last_committed_head
        .ok_or(ComputeError::LastCommittedHeadNotFound(parent))?;

    for event in events {
        match event {
            BlockEvent::Router(RouterEvent::BatchCommitted { digest }) => {
                last_committed_batch = *digest;
            }
            BlockEvent::Router(RouterEvent::HeadCommitted(head)) => {
                last_committed_head = *head;
            }
            BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, .. }) => {
                requested_codes.insert(*code_id);
                if db.code_valid(*code_id).is_none() {
                    missing_codes.insert(*code_id);
                }
            }
            BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, valid }) => {
                validated_codes.insert(*code_id);
                match db.code_valid(*code_id) {
                    None => {
                        missing_validated_codes.insert(*code_id);
                        missing_codes.insert(*code_id);
                    }
                    Some(local_status) if local_status != *valid => {
                        return Err(ComputeError::CodeValidationStatusMismatch {
                            code_id: *code_id,
                            local_status,
                            remote_status: *valid,
                        });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    db.mutate_block_meta(block, |meta| {
        meta.last_committed_batch = Some(last_committed_batch);
        meta.last_committed_head = Some(last_committed_head);
    });

    // Propagate `wait for code validation` blocks queue
    let mut codes_queue = db
        .block_codes_queue(parent)
        .ok_or(ComputeError::CodesQueueNotFound(parent))?;
    codes_queue.retain(|code_id| !validated_codes.contains(code_id));
    codes_queue.extend(requested_codes);
    db.set_block_codes_queue(block, codes_queue);

    Ok((missing_codes, missing_validated_codes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        Address, BlockHeader, Digest,
        db::{BlockMetaStorageWrite, CodesStorageWrite, OnChainStorageWrite},
        events::BlockEvent,
    };
    use ethexe_db::Database as DB;
    use gprimitives::{CodeId, H256};
    use nonempty::nonempty;
    use std::collections::VecDeque;

    /// Tests propagate_data_from_parent with empty events list
    #[test]
    fn test_propagate_data_from_parent_empty_events() {
        let db = DB::memory();
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);

        // Set initial data for parent block
        let initial_digest = Digest([42; 32]);
        let initial_head = H256::from([43; 32]);
        db.mutate_block_meta(parent_hash, |meta| {
            meta.last_committed_batch = Some(initial_digest);
            meta.last_committed_head = Some(initial_head);
        });
        db.set_block_codes_queue(parent_hash, VecDeque::new());
        db.set_validators(parent_hash, nonempty![Address::from([0u8; 20])]);

        let events = Vec::<BlockEvent>::new();

        let result =
            propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

        assert!(result.0.is_empty()); // missing_codes
        assert!(result.1.is_empty()); // missing_validated_codes

        // Verify that data was propagated from parent
        assert_eq!(
            db.block_meta(block_hash).last_committed_batch,
            Some(initial_digest)
        );
        assert_eq!(
            db.block_meta(block_hash).last_committed_head,
            Some(initial_head)
        );
        assert_eq!(db.block_codes_queue(block_hash), Some(VecDeque::new()));
    }

    /// Tests propagate_data_from_parent with BatchCommitted event
    #[test]
    fn test_propagate_data_from_parent_batch_committed() {
        let db = DB::memory();
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);

        // Set initial data for parent block
        let initial_digest = Digest([42; 32]);
        let initial_head = H256::from([43; 32]);
        db.mutate_block_meta(parent_hash, |meta| {
            meta.last_committed_batch = Some(initial_digest);
            meta.last_committed_head = Some(initial_head);
        });
        db.set_block_codes_queue(parent_hash, VecDeque::new());
        db.set_validators(parent_hash, nonempty![Address::from([0u8; 20])]);

        let new_digest = Digest([99; 32]);
        let events = [BlockEvent::Router(
            ethexe_common::events::RouterEvent::BatchCommitted { digest: new_digest },
        )];

        let result =
            propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

        assert!(result.0.is_empty());
        assert!(result.1.is_empty());

        // Verify that last_committed_batch was updated
        assert_eq!(
            db.block_meta(block_hash).last_committed_batch,
            Some(new_digest)
        );
        assert_eq!(
            db.block_meta(block_hash).last_committed_head,
            Some(initial_head)
        );
    }

    /// Tests propagate_data_from_parent with CodeValidationRequested for existing code
    #[test]
    fn test_propagate_data_from_parent_code_validation_requested_existing() {
        let db = DB::memory();
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);
        let code_id = CodeId::from([3; 32]);

        // Set initial data for parent block
        db.mutate_block_meta(parent_hash, |meta| {
            meta.last_committed_batch = Some(Digest([42; 32]));
            meta.last_committed_head = Some(H256::from([43; 32]));
        });
        db.set_block_codes_queue(parent_hash, VecDeque::new());
        db.set_validators(parent_hash, nonempty![Address::from([0u8; 20])]);

        // Add code to DB as valid
        db.set_code_valid(code_id, true);

        let events = [BlockEvent::Router(
            ethexe_common::events::RouterEvent::CodeValidationRequested {
                code_id,
                timestamp: 1000,
                tx_hash: H256::from([4; 32]),
            },
        )];

        let result =
            propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

        assert!(result.0.is_empty()); // missing_codes - code exists in DB
        assert!(result.1.is_empty()); // missing_validated_codes

        // Verify that code was added to queue
        let codes_queue = db.block_codes_queue(block_hash).unwrap();
        assert!(codes_queue.contains(&code_id));
    }

    /// Tests propagate_data_from_parent with CodeValidationRequested for missing code
    #[test]
    fn test_propagate_data_from_parent_code_validation_requested_missing() {
        let db = DB::memory();
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);
        let code_id = CodeId::from([3; 32]);

        // Set initial data for parent block
        db.mutate_block_meta(parent_hash, |meta| {
            meta.last_committed_batch = Some(Digest([42; 32]));
            meta.last_committed_head = Some(H256::from([43; 32]));
        });
        db.set_block_codes_queue(parent_hash, VecDeque::new());
        db.set_validators(parent_hash, nonempty![Address::from([0u8; 20])]);

        let events = [BlockEvent::Router(
            ethexe_common::events::RouterEvent::CodeValidationRequested {
                code_id,
                timestamp: 1000,
                tx_hash: H256::from([4; 32]),
            },
        )];

        let result =
            propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

        assert!(result.0.contains(&code_id)); // missing_codes
        assert!(result.1.is_empty()); // missing_validated_codes

        // Verify that code was added to queue
        let codes_queue = db.block_codes_queue(block_hash).unwrap();
        assert!(codes_queue.contains(&code_id));
    }

    /// Tests propagate_data_from_parent with CodeGotValidated for missing code
    #[test]
    fn test_propagate_data_from_parent_code_got_validated_missing() {
        let db = DB::memory();
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);
        let code_id = CodeId::from([3; 32]);

        // Set initial data for parent block
        db.mutate_block_meta(parent_hash, |meta| {
            meta.last_committed_batch = Some(Digest([42; 32]));
            meta.last_committed_head = Some(H256::from([43; 32]));
        });
        db.set_block_codes_queue(parent_hash, VecDeque::new());
        db.set_validators(parent_hash, nonempty![Address::from([0u8; 20])]);

        let events = [BlockEvent::Router(
            ethexe_common::events::RouterEvent::CodeGotValidated {
                code_id,
                valid: true,
            },
        )];

        let result =
            propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

        assert!(result.0.contains(&code_id)); // missing_codes
        assert!(result.1.contains(&code_id)); // missing_validated_codes
    }

    /// Tests propagate_data_from_parent with CodeGotValidated for existing code with matching status
    #[test]
    fn test_propagate_data_from_parent_code_got_validated_matching_status() {
        let db = DB::memory();
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);
        let code_id = CodeId::from([3; 32]);

        // Set initial data for parent block
        db.mutate_block_meta(parent_hash, |meta| {
            meta.last_committed_batch = Some(Digest([42; 32]));
            meta.last_committed_head = Some(H256::from([43; 32]));
        });
        db.set_block_codes_queue(parent_hash, VecDeque::new());
        db.set_validators(parent_hash, nonempty![Address::from([0u8; 20])]);

        // Add code to DB as valid
        db.set_code_valid(code_id, true);

        let events = [BlockEvent::Router(
            ethexe_common::events::RouterEvent::CodeGotValidated {
                code_id,
                valid: true,
            },
        )];

        let result =
            propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

        assert!(result.0.is_empty()); // missing_codes
        assert!(result.1.is_empty()); // missing_validated_codes
    }

    /// Tests propagate_data_from_parent with CodeGotValidated for existing code with mismatched status
    #[test]
    fn test_propagate_data_from_parent_code_got_validated_mismatched_status() {
        let db = DB::memory();
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);
        let code_id = CodeId::from([3; 32]);

        // Set initial data for parent block
        db.mutate_block_meta(parent_hash, |meta| {
            meta.last_committed_batch = Some(Digest([42; 32]));
            meta.last_committed_head = Some(H256::from([43; 32]));
        });
        db.set_block_codes_queue(parent_hash, VecDeque::new());

        // Add code to DB as valid
        db.set_code_valid(code_id, true);

        let events = [BlockEvent::Router(
            ethexe_common::events::RouterEvent::CodeGotValidated {
                code_id,
                valid: false, // mismatched status
            },
        )];

        let result = propagate_data_from_parent(&db, block_hash, parent_hash, events.iter());

        // Should return CodeValidationStatusMismatch error
        assert!(matches!(
            result,
            Err(ComputeError::CodeValidationStatusMismatch {
                code_id: err_code_id,
                local_status: true,
                remote_status: false,
            }) if err_code_id == code_id
        ));
    }

    /// Tests propagate_data_from_parent with other events (which are ignored)
    #[test]
    fn test_propagate_data_from_parent_other_events() {
        let db = DB::memory();
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);

        // Set initial data for parent block
        db.mutate_block_meta(parent_hash, |meta| {
            meta.last_committed_batch = Some(Digest([42; 32]));
            meta.last_committed_head = Some(H256::from([43; 32]));
        });
        db.set_block_codes_queue(parent_hash, VecDeque::new());
        db.set_validators(parent_hash, nonempty![Address::from([0u8; 20])]);

        let events = [
            BlockEvent::Router(
                ethexe_common::events::RouterEvent::ComputationSettingsChanged {
                    threshold: 100,
                    wvara_per_second: 200,
                },
            ),
            BlockEvent::Router(ethexe_common::events::RouterEvent::ProgramCreated {
                actor_id: gprimitives::ActorId::from([5; 32]),
                code_id: CodeId::from([6; 32]),
            }),
        ];

        let result =
            propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

        assert!(result.0.is_empty()); // missing_codes
        assert!(result.1.is_empty()); // missing_validated_codes
    }

    /// Tests propagate_data_from_parent with combination of events
    #[test]
    fn test_propagate_data_from_parent_combined_events() {
        let db = DB::memory();
        let block_hash = H256::from([2; 32]);
        let parent_hash = H256::from([1; 32]);
        let code_id1 = CodeId::from([3; 32]);
        let code_id2 = CodeId::from([4; 32]);
        let code_id3 = CodeId::from([5; 32]);

        // Set initial data for parent block
        db.mutate_block_meta(parent_hash, |meta| {
            meta.last_committed_batch = Some(Digest([42; 32]));
            meta.last_committed_head = Some(H256::from([43; 32]));
        });
        db.set_block_codes_queue(parent_hash, VecDeque::new());
        db.set_validators(parent_hash, nonempty![Address::from([0u8; 20])]);

        // Code2 already exists in DB
        db.set_code_valid(code_id2, true);

        let new_digest = Digest([99; 32]);
        let events = [
            BlockEvent::Router(ethexe_common::events::RouterEvent::BatchCommitted {
                digest: new_digest,
            }),
            BlockEvent::Router(
                ethexe_common::events::RouterEvent::CodeValidationRequested {
                    code_id: code_id1,
                    timestamp: 1000,
                    tx_hash: H256::from([7; 32]),
                },
            ),
            BlockEvent::Router(
                ethexe_common::events::RouterEvent::CodeValidationRequested {
                    code_id: code_id2,
                    timestamp: 1001,
                    tx_hash: H256::from([8; 32]),
                },
            ),
            BlockEvent::Router(ethexe_common::events::RouterEvent::CodeGotValidated {
                code_id: code_id3,
                valid: true,
            }),
        ];

        let result =
            propagate_data_from_parent(&db, block_hash, parent_hash, events.iter()).unwrap();

        // code_id1 - missing (requested but not found)
        // code_id3 - missing and validated (got validation but not found)
        assert_eq!(result.0.len(), 2); // missing_codes: code_id1, code_id3
        assert!(result.0.contains(&code_id1));
        assert!(result.0.contains(&code_id3));
        assert_eq!(result.1.len(), 1); // missing_validated_codes: code_id3
        assert!(result.1.contains(&code_id3));

        // Verify updates
        assert_eq!(
            db.block_meta(block_hash).last_committed_batch,
            Some(new_digest)
        );

        let codes_queue = db.block_codes_queue(block_hash).unwrap();
        assert!(codes_queue.contains(&code_id1));
        assert!(codes_queue.contains(&code_id2));
        assert!(!codes_queue.contains(&code_id3)); // this code was removed from queue
    }

    /// Tests prepare with empty chain of blocks
    #[test]
    fn test_prepare_empty_chain() {
        let db = DB::memory();
        let head = H256::from([10; 32]);

        // Create block as already prepared
        db.mutate_block_meta(head, |m| {
            m.synced = true;
            m.prepared = true; // block is already prepared
        });

        let result = prepare(&db, head).unwrap();

        assert!(result.chain.is_empty());
        assert!(result.missing_codes.is_empty());
        assert!(result.missing_validated_codes.is_empty());
    }

    /// Tests prepare with single block without events
    #[test]
    fn test_prepare_single_block_no_events() {
        let db = DB::memory();
        let parent_hash = H256::from([1; 32]);
        let head = H256::from([10; 32]);

        // Set initial data for parent block (required for propagate_data_from_parent)
        db.mutate_block_meta(parent_hash, |meta| {
            meta.last_committed_batch = Some(Digest([42; 32]));
            meta.last_committed_head = Some(H256::from([43; 32]));
        });
        db.set_block_codes_queue(parent_hash, VecDeque::new());
        db.set_validators(parent_hash, nonempty![Address::from([0u8; 20])]);

        // Configure parent as prepared
        db.mutate_block_meta(parent_hash, |m| {
            m.synced = true;
            m.prepared = true;
        });

        // Configure head as unprepared
        db.mutate_block_meta(head, |m| m.synced = true);

        let header = BlockHeader {
            height: 1,
            parent_hash,
            timestamp: 2000,
        };
        db.set_block_header(head, header);

        // Empty events
        db.set_block_events(head, &[]);

        let result = prepare(&db, head).unwrap();

        assert_eq!(result.chain.len(), 1);
        assert_eq!(result.chain[0].hash, head);
        assert_eq!(result.chain[0].header, header);
        assert!(result.missing_codes.is_empty());
        assert!(result.missing_validated_codes.is_empty());
    }

    /// Tests prepare with single block with events
    #[test]
    fn test_prepare_single_block_with_events() {
        let db = DB::memory();
        let parent_hash = H256::from([1; 32]);
        let head = H256::from([10; 32]);
        let code_id = CodeId::from([20; 32]);

        // Set initial data for parent block (required for propagate_data_from_parent)
        db.mutate_block_meta(parent_hash, |meta| {
            meta.last_committed_batch = Some(Digest([42; 32]));
            meta.last_committed_head = Some(H256::from([43; 32]));
        });
        db.set_block_codes_queue(parent_hash, VecDeque::new());
        db.set_validators(parent_hash, nonempty![Address::from([0u8; 20])]);

        // Configure parent as prepared
        db.mutate_block_meta(parent_hash, |m| {
            m.synced = true;
            m.prepared = true;
        });

        // Configure head as unprepared
        db.mutate_block_meta(head, |m| m.synced = true);

        let header = BlockHeader {
            height: 2,
            parent_hash,
            timestamp: 2000,
        };
        db.set_block_header(head, header);

        // Events with code validation request
        let events = vec![BlockEvent::Router(
            ethexe_common::events::RouterEvent::CodeValidationRequested {
                code_id,
                timestamp: 1000,
                tx_hash: H256::from([30; 32]),
            },
        )];
        db.set_block_events(head, &events);

        let result = prepare(&db, head).unwrap();

        assert_eq!(result.chain.len(), 1);
        assert_eq!(result.chain[0].hash, head);
        assert!(result.missing_codes.contains(&code_id));
        assert!(result.missing_validated_codes.is_empty());
    }

    /// Tests prepare with multiple blocks
    #[test]
    fn test_prepare_multiple_blocks() {
        let db = DB::memory();
        let grandparent_hash = H256::from([0; 32]);
        let parent_hash = H256::from([1; 32]);
        let head = H256::from([10; 32]);
        let code_id1 = CodeId::from([20; 32]);
        let code_id2 = CodeId::from([21; 32]);

        // Set initial data for grandparent block
        db.mutate_block_meta(grandparent_hash, |meta| {
            meta.last_committed_batch = Some(Digest([42; 32]));
            meta.last_committed_head = Some(H256::from([43; 32]));
        });
        db.set_block_codes_queue(grandparent_hash, VecDeque::new());
        db.set_validators(parent_hash, nonempty![Address::from([0u8; 20])]);

        // Configure grandparent as prepared
        db.mutate_block_meta(grandparent_hash, |m| {
            m.synced = true;
            m.prepared = true;
        });

        // Configure parent as unprepared
        db.mutate_block_meta(parent_hash, |m| m.synced = true);

        // Configure head as unprepared
        db.mutate_block_meta(head, |m| m.synced = true);

        let parent_header = BlockHeader {
            height: 1,
            parent_hash: grandparent_hash,
            timestamp: 1500,
        };
        db.set_block_header(parent_hash, parent_header);

        let head_header = BlockHeader {
            height: 2,
            parent_hash,
            timestamp: 2000,
        };
        db.set_block_header(head, head_header);

        // Events for parent block
        let parent_events = vec![BlockEvent::Router(
            ethexe_common::events::RouterEvent::CodeValidationRequested {
                code_id: code_id1,
                timestamp: 1000,
                tx_hash: H256::from([30; 32]),
            },
        )];
        db.set_block_events(parent_hash, &parent_events);

        // Events for head block
        let head_events = vec![BlockEvent::Router(
            ethexe_common::events::RouterEvent::CodeGotValidated {
                code_id: code_id2,
                valid: true,
            },
        )];
        db.set_block_events(head, &head_events);

        let result = prepare(&db, head).unwrap();

        assert_eq!(result.chain.len(), 2);
        // Blocks should be sorted from old to new
        assert_eq!(result.chain[0].hash, parent_hash);
        assert_eq!(result.chain[1].hash, head);

        // Check missing codes from both blocks
        assert!(result.missing_codes.contains(&code_id1)); // from parent
        assert!(result.missing_codes.contains(&code_id2)); // from head
        assert!(result.missing_validated_codes.contains(&code_id2)); // only from head
    }

    /// Tests prepare when block events are not found in DB
    #[test]
    fn test_prepare_missing_block_events() {
        let db = DB::memory();
        let parent_hash = H256::from([1; 32]);
        let head = H256::from([10; 32]);

        // Configure parent as prepared
        db.mutate_block_meta(parent_hash, |m| {
            m.synced = true;
            m.prepared = true;
        });

        // Configure head as unprepared
        db.mutate_block_meta(head, |m| m.synced = true);

        let header = BlockHeader {
            height: 1,
            parent_hash,
            timestamp: 2000,
        };
        db.set_block_header(head, header);

        // DO NOT set events for block

        let result = prepare(&db, head);

        // Should return BlockEventsNotFound error
        assert!(matches!(
            result,
            Err(ComputeError::BlockEventsNotFound(block_hash)) if block_hash == head
        ));
    }

    /// Tests prepare with error from propagate_data_from_parent
    #[test]
    fn test_prepare_propagation_error() {
        let db = DB::memory();
        let parent_hash = H256::from([1; 32]);
        let head = H256::from([10; 32]);
        let code_id = CodeId::from([20; 32]);

        // Set initial data for parent block (required for propagate_data_from_parent)
        db.mutate_block_meta(parent_hash, |meta| {
            meta.last_committed_batch = Some(Digest([42; 32]));
            meta.last_committed_head = Some(H256::from([43; 32]));
        });
        db.set_block_codes_queue(parent_hash, VecDeque::new());

        // Configure parent as prepared
        db.mutate_block_meta(parent_hash, |m| {
            m.synced = true;
            m.prepared = true;
        });

        // Configure head as unprepared
        db.mutate_block_meta(head, |m| m.synced = true);

        let header = BlockHeader {
            height: 1,
            parent_hash,
            timestamp: 2000,
        };
        db.set_block_header(head, header);

        // Add code to DB as valid
        db.set_code_valid(code_id, true);

        // Events with mismatched validation status
        let events = [BlockEvent::Router(
            ethexe_common::events::RouterEvent::CodeGotValidated {
                code_id,
                valid: false, // mismatched status
            },
        )];
        db.set_block_events(head, &events);

        let result = prepare(&db, head);

        // Should return error from propagate_data_from_parent
        assert!(matches!(
            result,
            Err(ComputeError::CodeValidationStatusMismatch { .. })
        ));
    }
}
