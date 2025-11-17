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
    Database,
    iterator::{ChainNode, DatabaseIteratorError, DatabaseIteratorStorage},
    visitor::{DatabaseVisitor, walk},
};
use ethexe_common::{
    Announce, BlockHeader, HashOf, ScheduledTask,
    db::{AnnounceMeta, AnnounceStorageRO, BlockMeta, BlockMetaStorageRO, OnChainStorageRO},
};
use ethexe_runtime_common::state::{MessageQueue, MessageQueueHashWithSize};
use gear_core::code::CodeMetadata;
use gprimitives::{CodeId, H256};
use parity_scale_codec::Encode;
use std::{collections::BTreeSet, hash::Hash};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum IntegrityVerifierError {
    DatabaseIterator(DatabaseIteratorError),

    /* block */
    BlockIsNotSynced(H256),
    BlockIsNotPrepared(H256),
    BlockAnnouncesLenNotOne(H256),
    NoBlockLastCommittedBatch(H256),
    NoBlockLastCommittedAnnounce(H256),
    NoBlockAnnounces(H256),
    NoBlockHeader(H256),

    /* announce */
    AnnounceNotFound(HashOf<Announce>),
    AnnounceIsNotComputed(HashOf<Announce>),
    AnnounceIsNotIncluded(HashOf<Announce>),
    AnnounceOffChainTransactionsNotEmpty(HashOf<Announce>),

    /* block header */
    NoParentBlockHeader(H256),
    InvalidBlockParentHeight {
        parent_height: u32,
        height: u32,
    },
    InvalidParentTimestamp {
        parent_timestamp: u64,
        timestamp: u64,
    },

    /* code */
    CodeIsNotValid,
    InvalidCodeLenInMetadata {
        code_id: CodeId,
        metadata_len: u32,
        original_len: u32,
    },

    /* rest */
    AnnounceScheduleHasExpiredTasks {
        announce_hash: HashOf<Announce>,
        expiry: u32,
        tasks: usize,
    },
    InvalidCachedMessageQueueSize {
        hash: HashOf<MessageQueue>,
        cached_size: u8,
        actual_size: u8,
    },
}

pub struct IntegrityVerifier {
    db: Database,
    errors: Vec<IntegrityVerifierError>,
    message_queue_size: Option<u8>,
    original_code: Option<Vec<u8>>,
    bottom: Option<H256>,
}

impl IntegrityVerifier {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            errors: Vec::new(),
            message_queue_size: None,
            original_code: None,
            bottom: None,
        }
    }

    pub fn verify_chain(
        mut self,
        head: H256,
        bottom: H256,
    ) -> Result<(), Vec<IntegrityVerifierError>> {
        self.bottom = Some(bottom);
        walk(&mut self, ChainNode { head, bottom });

        #[cfg(debug_assertions)]
        {
            use std::collections::HashSet;

            self.errors
                .clone()
                .into_iter()
                .fold(HashSet::new(), |mut set, error| {
                    assert!(set.insert(error), "Duplicate error: {error:?}");
                    set
                });
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors)
        }
    }
}

impl DatabaseVisitor for IntegrityVerifier {
    fn db(&self) -> &dyn DatabaseIteratorStorage {
        &self.db
    }

    fn clone_boxed_db(&self) -> Box<dyn DatabaseIteratorStorage> {
        Box::new(self.db.clone())
    }

    fn on_db_error(&mut self, error: DatabaseIteratorError) {
        self.errors
            .push(IntegrityVerifierError::DatabaseIterator(error));
    }

    fn visit_block_meta(&mut self, block: H256, meta: BlockMeta) {
        if !meta.prepared {
            self.errors
                .push(IntegrityVerifierError::BlockIsNotPrepared(block));
        }
        if meta.last_committed_batch.is_none() {
            self.errors
                .push(IntegrityVerifierError::NoBlockLastCommittedBatch(block));
        }
        if meta.last_committed_announce.is_none() {
            self.errors
                .push(IntegrityVerifierError::NoBlockLastCommittedAnnounce(block));
        }
        if let Some(announces) = meta.announces {
            if announces.len() != 1 {
                self.errors
                    .push(IntegrityVerifierError::BlockAnnouncesLenNotOne(block));
            }
        } else {
            self.errors
                .push(IntegrityVerifierError::NoBlockAnnounces(block));
        }
    }

    fn visit_announce(&mut self, announce_hash: HashOf<Announce>, announce: Announce) {
        if !announce.injected_transactions.is_empty() {
            self.errors
                .push(IntegrityVerifierError::AnnounceOffChainTransactionsNotEmpty(announce_hash));
        }
        if self
            .db
            .block_meta(announce.block_hash)
            .announces
            .map(|announces| announces.iter().all(|a| *a != announce_hash))
            .unwrap_or(true)
        {
            self.errors
                .push(IntegrityVerifierError::AnnounceIsNotIncluded(announce_hash));
        }
    }

    fn visit_announce_meta(
        &mut self,
        announce_hash: HashOf<Announce>,
        announce_meta: AnnounceMeta,
    ) {
        if !announce_meta.computed {
            self.errors
                .push(IntegrityVerifierError::AnnounceIsNotComputed(announce_hash));
        }
    }

    fn visit_block_synced(&mut self, block: H256, block_synced: bool) {
        if !block_synced {
            self.errors
                .push(IntegrityVerifierError::BlockIsNotSynced(block));
        }
    }

    fn visit_block_header(&mut self, block: H256, header: BlockHeader) {
        let Some(parent_header) = self.db().block_header(header.parent_hash) else {
            if self.bottom == Some(block) {
                // it's not guaranteed bottom parent block has header
                return;
            }

            self.errors
                .push(IntegrityVerifierError::NoParentBlockHeader(
                    header.parent_hash,
                ));
            return;
        };

        if parent_header.height + 1 != header.height {
            self.errors
                .push(IntegrityVerifierError::InvalidBlockParentHeight {
                    parent_height: parent_header.height,
                    height: header.height,
                });
        }

        if parent_header.timestamp > header.timestamp {
            self.errors
                .push(IntegrityVerifierError::InvalidParentTimestamp {
                    parent_timestamp: parent_header.timestamp,
                    timestamp: header.timestamp,
                });
        }
    }

    fn visit_code_valid(&mut self, _code_id: CodeId, code_valid: bool) {
        if !code_valid {
            self.errors.push(IntegrityVerifierError::CodeIsNotValid);
        }
    }

    fn visit_original_code(&mut self, original_code: Vec<u8>) {
        self.original_code = Some(original_code.to_vec());
    }

    fn visit_code_metadata(&mut self, code_id: CodeId, metadata: CodeMetadata) {
        let original_code = self.original_code.take();
        if let Some(original_code) = original_code
            && metadata.original_code_len() != original_code.len() as u32
        {
            self.errors
                .push(IntegrityVerifierError::InvalidCodeLenInMetadata {
                    code_id,
                    metadata_len: metadata.original_code_len(),
                    original_len: original_code.len() as u32,
                });
        }
    }

    fn visit_announce_schedule_tasks(
        &mut self,
        announce_hash: HashOf<Announce>,
        height: u32,
        tasks: BTreeSet<ScheduledTask>,
    ) {
        let Some(announce) = self.db.announce(announce_hash) else {
            self.errors
                .push(IntegrityVerifierError::AnnounceNotFound(announce_hash));
            return;
        };
        let Some(header) = self.db.block_header(announce.block_hash) else {
            self.errors
                .push(IntegrityVerifierError::NoBlockHeader(announce.block_hash));
            return;
        };
        if height <= header.height {
            self.errors
                .push(IntegrityVerifierError::AnnounceScheduleHasExpiredTasks {
                    announce_hash,
                    expiry: height,
                    tasks: tasks.len(),
                });
        }
    }

    fn visit_message_queue_hash_with_size(
        &mut self,
        queue_hash_with_size: MessageQueueHashWithSize,
    ) {
        if let Some(_hash) = queue_hash_with_size.hash.to_inner() {
            self.message_queue_size = Some(queue_hash_with_size.cached_queue_size);
        }
    }

    fn visit_message_queue(&mut self, queue: MessageQueue) {
        let encoded_queue = queue.encode();
        let hash = crate::hash(&encoded_queue);
        let hash = unsafe { HashOf::new(hash) };

        let cached_queue_size = self.message_queue_size.take().expect(
            "`visit_message_queue_hash_with_size` must be called before `visit_message_queue`",
        );
        if cached_queue_size != queue.len() as u8 {
            self.errors
                .push(IntegrityVerifierError::InvalidCachedMessageQueueSize {
                    hash,
                    cached_size: cached_queue_size,
                    actual_size: queue.len() as u8,
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iterator::{
        AnnounceScheduleTasksNode, BlockNode, CodeIdNode, MessageQueueHashWithSizeNode,
        MessageQueueNode, tests::setup_db,
    };
    use ethexe_common::{
        Digest, MaybeHashOf, ProgramStates, Schedule,
        db::{AnnounceStorageRW, BlockMetaStorageRW, CodesStorageRW, OnChainStorageRW},
    };
    use ethexe_runtime_common::state::Storage;
    use gear_core::{
        code::{CodeMetadata, InstantiatedSectionSizes, InstrumentationStatus, InstrumentedCode},
        pages::WasmPagesAmount,
    };

    #[test]
    fn test_block_meta_not_synced_error() {
        let db = setup_db();
        let block = H256::random();

        // Insert block with not synced meta
        db.mutate_block_meta(block, |meta| {
            meta.prepared = true;
        });

        let mut verifier = IntegrityVerifier::new(db);
        walk(&mut verifier, BlockNode { block });
        assert!(
            verifier
                .errors
                .contains(&IntegrityVerifierError::BlockIsNotSynced(block))
        );
    }

    #[test]
    fn test_block_meta_not_prepared_error() {
        let db = setup_db();
        let block = H256::random();

        // Insert block with not prepared meta
        db.mutate_block_meta(block, |meta| {
            meta.prepared = false;
        });

        let mut verifier = IntegrityVerifier::new(db);
        walk(&mut verifier, BlockNode { block });
        assert!(
            verifier
                .errors
                .contains(&IntegrityVerifierError::BlockIsNotPrepared(block))
        );
    }

    #[test]
    fn test_no_parent_block_header_error() {
        let db = setup_db();
        let block = H256::random();
        let parent_hash = H256::random();

        // Insert valid meta but header with non-existent parent
        db.mutate_block_meta(block, |meta| {
            meta.prepared = true;
        });

        let header = BlockHeader {
            height: 1,
            parent_hash,
            timestamp: 1000,
        };
        db.set_block_header(block, header);

        let mut verifier = IntegrityVerifier::new(db);
        walk(&mut verifier, BlockNode { block });
        assert!(
            verifier
                .errors
                .contains(&IntegrityVerifierError::NoParentBlockHeader(parent_hash))
        );
    }

    #[test]
    fn test_invalid_block_parent_height_error() {
        let db = setup_db();
        let block = H256::random();
        let parent_hash = H256::random();

        // Setup parent block
        db.mutate_block_meta(parent_hash, |meta| {
            meta.prepared = true;
        });

        let parent_hash1 = H256::zero();
        let parent_header = BlockHeader {
            height: 5,
            parent_hash: parent_hash1,
            timestamp: 1000,
        };
        db.set_block_header(parent_hash, parent_header);

        // Setup child block with invalid height
        db.mutate_block_meta(block, |meta| {
            meta.prepared = true;
        });

        let header = BlockHeader {
            height: 10,
            parent_hash,
            timestamp: 2000,
        }; // Should be 6, not 10
        db.set_block_header(block, header);

        let mut verifier = IntegrityVerifier::new(db);
        walk(&mut verifier, BlockNode { block });
        assert!(
            verifier
                .errors
                .contains(&IntegrityVerifierError::InvalidBlockParentHeight {
                    parent_height: 5,
                    height: 10,
                })
        );
    }

    #[test]
    fn test_invalid_parent_timestamp_error() {
        let db = setup_db();
        let block = H256::random();
        let parent_hash = H256::random();

        // Setup parent block
        db.mutate_block_meta(parent_hash, |meta| {
            meta.prepared = true;
        });

        let parent_hash1 = H256::zero();
        let parent_header = BlockHeader {
            height: 5,
            parent_hash: parent_hash1,
            timestamp: 2000,
        };
        db.set_block_header(parent_hash, parent_header);

        // Setup child block with earlier timestamp
        db.mutate_block_meta(parent_hash, |meta| {
            meta.prepared = true;
        });
        let header = BlockHeader {
            height: 6,
            parent_hash,
            timestamp: 1000,
        }; // Earlier than parent
        db.set_block_header(block, header);

        let mut verifier = IntegrityVerifier::new(db);
        walk(&mut verifier, BlockNode { block });
        assert!(
            verifier
                .errors
                .contains(&IntegrityVerifierError::InvalidParentTimestamp {
                    parent_timestamp: 2000,
                    timestamp: 1000,
                })
        );
    }

    #[test]
    fn test_code_is_not_valid_error() {
        let db = setup_db();
        let code_id = CodeId::from(1);

        // Set code as invalid
        db.set_code_valid(code_id, false);

        let mut verifier = IntegrityVerifier::new(db);
        walk(&mut verifier, CodeIdNode { code_id });

        assert!(
            verifier
                .errors
                .contains(&IntegrityVerifierError::CodeIsNotValid)
        );
    }

    #[test]
    fn test_invalid_code_len_in_metadata_error() {
        const ORIGINAL_CODE: &[u8] = &[1, 2, 3, 4];

        let db = setup_db();

        let metadata = CodeMetadata::new(
            10,
            BTreeSet::default(),
            WasmPagesAmount::from(0),
            None,
            InstrumentationStatus::NotInstrumented,
        ); // Wrong length: 10 instead of 4

        // Set up all required code data with mismatched length
        let code_id = db.set_original_code(ORIGINAL_CODE);
        db.set_code_valid(code_id, true);
        db.set_instrumented_code(
            ethexe_runtime_common::VERSION,
            code_id,
            InstrumentedCode::new(
                vec![1, 2, 3, 4, 5],
                InstantiatedSectionSizes::new(0, 0, 0, 0, 0, 0),
            ),
        );
        db.set_code_metadata(code_id, metadata);

        let mut verifier = IntegrityVerifier::new(db);
        walk(&mut verifier, CodeIdNode { code_id });

        assert_eq!(
            verifier.errors,
            [IntegrityVerifierError::InvalidCodeLenInMetadata {
                code_id,
                metadata_len: 10,
                original_len: ORIGINAL_CODE.len() as u32,
            }]
        );
    }

    #[test]
    fn test_block_schedule_has_expired_tasks_error() {
        let db = setup_db();
        let block_hash = H256::random();

        let announce = Announce::base(block_hash, HashOf::zero());
        let announce_hash = db.set_announce(announce);

        // Setup block with height 100
        let parent_hash = H256::zero();
        let header = BlockHeader {
            height: 100,
            parent_hash,
            timestamp: 1000,
        };
        db.set_block_header(block_hash, header);

        // Create tasks scheduled for height 50 (expired)
        let mut verifier = IntegrityVerifier::new(db);
        walk(
            &mut verifier,
            AnnounceScheduleTasksNode {
                announce_hash,
                height: 50,
                tasks: BTreeSet::new(),
            },
        );

        assert!(verifier.errors.contains(
            &IntegrityVerifierError::AnnounceScheduleHasExpiredTasks {
                announce_hash,
                expiry: 50,
                tasks: 0,
            }
        ));
    }

    #[test]
    fn test_visit_message_queue_invalid_cached_size() {
        let db = setup_db();
        let mut verifier = IntegrityVerifier::new(db.clone());

        // Create a message queue with some messages
        let queue = MessageQueue::default();
        let hash = db.write_message_queue(queue.clone());

        // Cache with wrong size (actual queue is empty, but we cache size as 5)
        let queue_hash_with_size = MessageQueueHashWithSize {
            hash: MaybeHashOf::from(Some(hash)),
            cached_queue_size: 5, // Wrong size
        };

        walk(
            &mut verifier,
            MessageQueueHashWithSizeNode {
                queue_hash_with_size,
            },
        );

        // Should have an error about invalid cached size
        assert_eq!(
            verifier.errors,
            [IntegrityVerifierError::InvalidCachedMessageQueueSize {
                hash,
                cached_size: 5,
                actual_size: 0,
            }]
        );
    }

    #[test]
    #[should_panic(
        expected = "`visit_message_queue_hash_with_size` must be called before `visit_message_queue`"
    )]
    fn test_visit_message_queue_without_hash_panics() {
        let db = setup_db();
        let mut verifier = IntegrityVerifier::new(db);

        // Create a message queue
        let message_queue = MessageQueue::default();

        // Try to visit message queue without first calling visit_message_queue_hash_with_size
        // This should panic
        walk(&mut verifier, MessageQueueNode { message_queue });
    }

    #[test]
    fn test_visit_message_queue_success() {
        let db = setup_db();
        let mut verifier = IntegrityVerifier::new(db.clone());

        let queue = MessageQueue::default();
        let hash = db.write_message_queue(queue.clone());

        let queue_hash_with_size = MessageQueueHashWithSize {
            hash: MaybeHashOf::from(Some(hash)),
            cached_queue_size: queue.len() as u8,
        };

        walk(
            &mut verifier,
            MessageQueueHashWithSizeNode {
                queue_hash_with_size,
            },
        );

        assert_eq!(verifier.message_queue_size, None);
        assert_eq!(verifier.errors, []);
    }

    #[test]
    fn test_multiple_errors_collected() {
        let db = setup_db();
        let block_hash = H256::random();

        // Insert block with multiple issues
        db.mutate_block_meta(block_hash, |meta| {
            meta.prepared = false;
        });

        let verifier = IntegrityVerifier::new(db);
        let errors = verifier.verify_chain(block_hash, block_hash).unwrap_err();
        assert!(errors.contains(&IntegrityVerifierError::BlockIsNotSynced(block_hash)));
        assert!(errors.contains(&IntegrityVerifierError::BlockIsNotPrepared(block_hash)));
        assert!(errors.len() >= 2);
    }

    #[test]
    fn test_successful_verification_with_valid_data() {
        let db = setup_db();
        let block_hash = H256::random();
        let parent_hash = H256::zero();
        let block_header = BlockHeader {
            height: 100,
            parent_hash,
            timestamp: 1000,
        };

        let announce = Announce::base(block_hash, HashOf::zero());
        let announce_hash = db.set_announce(announce);
        db.set_announce_program_states(announce_hash, ProgramStates::new());
        db.set_announce_schedule(announce_hash, Schedule::new());
        db.set_announce_outcome(announce_hash, Vec::new());
        db.mutate_announce_meta(announce_hash, |meta| {
            meta.computed = true;
        });

        db.set_block_header(block_hash, block_header);
        db.set_block_events(block_hash, &[]);
        db.mutate_block_meta(block_hash, |meta| {
            meta.prepared = true;
            meta.last_committed_batch = Some(Digest::random());
            meta.last_committed_announce = Some(announce_hash);
            meta.announces = Some([announce_hash].into());
            meta.codes_queue = Some(Default::default());
        });
        db.set_block_synced(block_hash);

        let verifier = IntegrityVerifier::new(db);
        verifier.verify_chain(block_hash, block_hash).unwrap();
    }

    #[test]
    fn test_database_visitor_error_propagation() {
        let db = setup_db();
        let verifier = IntegrityVerifier::new(db);

        // This should trigger DatabaseVisitorError due to missing block
        let non_existent_block = H256::random();
        let errors = verifier
            .verify_chain(non_existent_block, non_existent_block)
            .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, IntegrityVerifierError::DatabaseIterator(_)))
        );
    }
}
