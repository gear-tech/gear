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

use crate::{ComputeError, ComputeEvent, Result, service::SubService};
use ethexe_common::{
    BlockData,
    db::{
        BlockMetaStorageRO, BlockMetaStorageRW, CodesStorageRO, LatestDataStorageRW,
        OnChainStorageRO, OnChainStorageRW,
    },
    events::{
        BlockEvent, RouterEvent,
        router::{
            AnnouncesCommittedEvent, BatchCommittedEvent, CodeGotValidatedEvent,
            CodeValidationRequestedEvent, ValidatorsCommittedForEraEvent,
        },
    },
};
use ethexe_db::Database;
use gprimitives::{CodeId, H256};
use metrics::Gauge;
use std::{
    collections::{HashSet, VecDeque},
    task::{Context, Poll},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    BlockPrepared(H256),
    RequestCodes(HashSet<CodeId>),
}

impl From<Event> for ComputeEvent {
    fn from(event: Event) -> Self {
        match event {
            Event::BlockPrepared(hash) => ComputeEvent::BlockPrepared(hash),
            Event::RequestCodes(codes) => ComputeEvent::RequestLoadCodes(codes),
        }
    }
}

enum State {
    Start,
    WaitingForBlock,
    WaitingForCodes {
        codes: HashSet<CodeId>,
        not_prepared_blocks_chain: VecDeque<BlockData>,
    },
}

/// Metrics for the [`PrepareSubService`].
#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_compute:prepare")]
struct Metrics {
    /// Number of codes waiting for loading to advance block processing
    pub waiting_codes_count: Gauge,
    /// Number of blocks in the queue for processing
    pub blocks_queue_len: Gauge,
}

pub struct PrepareSubService {
    db: Database,
    state: State,
    input: VecDeque<H256>,
    metrics: Metrics,
}

impl PrepareSubService {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            state: State::Start,
            input: VecDeque::new(),
            metrics: Metrics::default(),
        }
    }

    pub fn receive_block_to_prepare(&mut self, block: H256) {
        self.metrics.blocks_queue_len.increment(1);

        self.input.push_back(block);
    }

    pub fn receive_processed_code(&mut self, code_id: CodeId) {
        if let State::WaitingForCodes { codes, .. } = &mut self.state
            && codes.remove(&code_id)
        {
            self.metrics.waiting_codes_count.decrement(1);
        }
    }
}

impl SubService for PrepareSubService {
    type Output = Event;

    fn poll_next(&mut self, _cx: &mut Context<'_>) -> Poll<Result<Self::Output>> {
        if matches!(&self.state, State::WaitingForBlock | State::Start) {
            // Use pop_back to prepare the most recent blocks first,
            // this is the most efficient way of preparing blocks in case of multiple pending blocks.
            let Some(block_hash) = self.input.pop_back() else {
                return Poll::Pending;
            };
            self.metrics.blocks_queue_len.decrement(1);

            if !self.db.block_synced(block_hash) {
                return Poll::Ready(Err(ComputeError::BlockNotSynced(block_hash)));
            }

            let not_prepared_blocks_chain =
                collect_not_prepared_blocks_chain(&self.db, block_hash)?;

            if not_prepared_blocks_chain.is_empty() {
                // Block is already prepared
                return Poll::Ready(Ok(Event::BlockPrepared(block_hash)));
            }

            log::trace!("Collected a chain to prepare {not_prepared_blocks_chain:?}");

            let MissingData {
                codes,
                validated_codes,
            } = missing_data(
                &self.db,
                &not_prepared_blocks_chain,
                matches!(&self.state, State::Start),
            )?;

            self.metrics
                .waiting_codes_count
                .set(validated_codes.len() as f64);

            self.state = State::WaitingForCodes {
                codes: validated_codes,
                not_prepared_blocks_chain,
            };

            if !codes.is_empty() {
                return Poll::Ready(Ok(Event::RequestCodes(codes)));
            }
        }

        if let State::WaitingForCodes {
            codes,
            not_prepared_blocks_chain,
        } = &mut self.state
            && codes.is_empty()
        {
            log::trace!("All validated codes are processed, start to prepare blocks");

            let head = not_prepared_blocks_chain
                .back()
                .unwrap_or_else(|| unreachable!("chain must be non-empty"))
                .hash;

            for block in std::mem::take(not_prepared_blocks_chain) {
                prepare_one_block(&self.db, block)?;
            }

            self.state = State::WaitingForBlock;

            return Poll::Ready(Ok(Event::BlockPrepared(head)));
        }

        Poll::Pending
    }
}

/// Collects a chain of blocks that are not yet prepared, starting from `block_hash`
/// and going backwards through parent hashes until a prepared block is found.
/// Returns the collected blocks in a `VecDeque`, ordered from oldest to newest.
fn collect_not_prepared_blocks_chain(
    db: &Database,
    mut block_hash: H256,
) -> Result<VecDeque<BlockData>> {
    let mut chain = VecDeque::new();

    loop {
        if db.block_meta(block_hash).prepared {
            break;
        }

        let header = db
            .block_header(block_hash)
            .ok_or(ComputeError::BlockHeaderNotFound(block_hash))?;
        let events = db
            .block_events(block_hash)
            .ok_or(ComputeError::BlockEventsNotFound(block_hash))?;

        chain.push_front(BlockData {
            hash: block_hash,
            header,
            events,
        });

        block_hash = header.parent_hash;
    }

    Ok(chain)
}

#[derive(Debug)]
struct MissingData {
    codes: HashSet<CodeId>,
    validated_codes: HashSet<CodeId>,
}

/// Collect codes that does not have validation status in the database from the given chain of blocks.
/// If `is_start` is true, also consider codes requested in the parent block of the first block in the chain.
/// Note: consider code as "missing" even if its original bytes are present in the database,
/// but its validation status is not known. Blob-loader wouldn't load such codes, but would emit event
/// that code is loaded and then processing would start.
fn missing_data(db: &Database, chain: &VecDeque<BlockData>, is_start: bool) -> Result<MissingData> {
    let mut missing_codes = HashSet::new();
    let mut missing_validated_codes = HashSet::new();

    if is_start {
        // If this is the first call for collecting missing data, then we must take into account codes,
        // that were requested in the parent block, but does not loaded in previous node execution.

        // Note: fast_sync does not recover codes queue for start block, because codes queue is propagated information.
        // This is not a big problem, because if this node starts with fast_sync then it means,
        // that there are another nodes in the network, which soon or later will commit codes validation status,
        // and this node will be able to load missing codes from them.

        let Some(parent_block_hash) = chain.front().map(|b| b.header.parent_hash) else {
            // no blocks
            return Ok(MissingData {
                codes: missing_codes,
                validated_codes: missing_validated_codes,
            });
        };

        missing_codes.extend(
            db.block_meta(parent_block_hash)
                .codes_queue
                .ok_or(ComputeError::BlockNotPrepared(parent_block_hash))?
                .into_iter()
                .filter(|code_id| db.code_valid(*code_id).is_none()),
        );
    }

    for block in chain {
        for event in &block.events {
            match event {
                BlockEvent::Router(RouterEvent::CodeValidationRequested(
                    CodeValidationRequestedEvent { code_id, .. },
                )) if db.code_valid(*code_id).is_none() => {
                    missing_codes.insert(*code_id);
                }
                BlockEvent::Router(RouterEvent::CodeGotValidated(CodeGotValidatedEvent {
                    code_id,
                    ..
                })) if db.code_valid(*code_id).is_none() => {
                    missing_validated_codes.insert(*code_id);
                    missing_codes.insert(*code_id);
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

fn prepare_one_block<DB: BlockMetaStorageRW + LatestDataStorageRW + OnChainStorageRW>(
    db: &DB,
    block: BlockData,
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
    let mut latest_validators_committed_era = db
        .block_validators_committed_for_era(parent)
        .unwrap_or_else(|| {
            // TODO: !!! temporary fix
            let tl = db.protocol_timelines().expect("must be");
            tl.era_from_ts(block.header.timestamp)
        });

    for event in block.events {
        match event {
            BlockEvent::Router(RouterEvent::BatchCommitted(BatchCommittedEvent { digest })) => {
                last_committed_batch = digest;
            }
            BlockEvent::Router(RouterEvent::CodeValidationRequested(
                CodeValidationRequestedEvent { code_id, .. },
            )) => {
                requested_codes.insert(code_id);
            }
            BlockEvent::Router(RouterEvent::CodeGotValidated(CodeGotValidatedEvent {
                code_id,
                ..
            })) => {
                validated_codes.insert(code_id);
            }
            BlockEvent::Router(RouterEvent::AnnouncesCommitted(head)) => {
                last_committed_announce_hash = Some(head);
            }

            BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra(
                ValidatorsCommittedForEraEvent { era_index },
            )) => {
                // TODO !!! kuzmindev: here must be `if era_index != latest_validators_committed_era + 1`
                if era_index < latest_validators_committed_era {
                    return Err(ComputeError::ValidatorsCommittedForEarlierEra {
                        previous_commitment_era_index: latest_validators_committed_era,
                        commitment_era_index: era_index,
                    });
                }

                latest_validators_committed_era = era_index;
            }
            _ => {}
        }
    }

    codes_queue.retain(|code_id| !validated_codes.contains(code_id));
    codes_queue.extend(requested_codes);

    let last_committed_announce_hash =
        if let Some(AnnouncesCommittedEvent(hash)) = last_committed_announce_hash {
            hash
        } else {
            parent_meta
                .last_committed_announce
                .ok_or(ComputeError::LastCommittedHeadNotFound(parent))?
        };

    db.mutate_block_meta(block.hash, |meta| {
        meta.last_committed_batch = Some(last_committed_batch);
        meta.codes_queue = Some(codes_queue);
        meta.last_committed_announce = Some(last_committed_announce_hash);
        meta.prepared = true;
    });

    db.mutate_latest_data(|data| {
        data.prepared_block_hash = block.hash;
    })
    .ok_or(ComputeError::LatestDataNotFound)?;

    db.set_block_validators_committed_for_era(block.hash, latest_validators_committed_era);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{Announce, Digest, HashOf, events::BlockEvent, mock::*};
    use ethexe_db::Database;
    use gear_core::ids::prelude::CodeIdExt;
    use gprimitives::H256;

    #[test]
    fn test_prepare_one_block() {
        gear_utils::init_default_logger();

        let db = Database::memory();
        let chain = BlockChain::mock(1).setup(&db);

        let code1_id = CodeId::from([1u8; 32]);
        let code2_id = CodeId::from([2u8; 32]);
        let batch_committed = Digest::random();

        let block1_announce_hash = HashOf::<Announce>::random();

        let block = chain.blocks[1].to_simple().next_block();
        let block = BlockData {
            hash: block.hash,
            header: block.header,
            events: vec![
                BlockEvent::Router(RouterEvent::BatchCommitted(BatchCommittedEvent {
                    digest: batch_committed,
                })),
                BlockEvent::Router(RouterEvent::AnnouncesCommitted(AnnouncesCommittedEvent(
                    block1_announce_hash,
                ))),
                BlockEvent::Router(RouterEvent::CodeGotValidated(CodeGotValidatedEvent {
                    code_id: code1_id,
                    valid: true,
                })),
                BlockEvent::Router(RouterEvent::CodeValidationRequested(
                    CodeValidationRequestedEvent {
                        code_id: code2_id,
                        timestamp: 1000,
                        tx_hash: H256::random(),
                    },
                )),
            ],
        }
        .setup(&db);

        prepare_one_block(&db, block.clone()).unwrap();

        let meta = db.block_meta(block.hash);
        assert!(meta.prepared);
        assert_eq!(meta.codes_queue, Some(vec![code2_id].into()),);
        assert_eq!(meta.last_committed_batch, Some(batch_committed),);
        assert_eq!(meta.last_committed_announce, Some(block1_announce_hash));
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn test_prepare_no_codes() {
        gear_utils::init_default_logger();

        let db = Database::memory();
        let mut service = PrepareSubService::new(db.clone());
        let chain = BlockChain::mock(1).setup(&db);
        let block = chain.blocks[1].to_simple().next_block().setup(&db);

        service.receive_block_to_prepare(block.hash);

        assert_eq!(
            service.next().await.unwrap(),
            Event::BlockPrepared(block.hash),
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn test_prepare_with_codes() {
        gear_utils::init_default_logger();

        let db = Database::memory();
        let mut service = PrepareSubService::new(db.clone());
        let chain = BlockChain::mock(1).setup(&db);

        let code1_id = CodeId::from([1u8; 32]);
        let code2_id = CodeId::from([2u8; 32]);

        let block = chain.blocks[1].to_simple().next_block();
        let block = BlockData {
            hash: block.hash,
            header: block.header,
            events: vec![
                BlockEvent::Router(RouterEvent::CodeGotValidated(CodeGotValidatedEvent {
                    code_id: code1_id,
                    valid: true,
                })),
                BlockEvent::Router(RouterEvent::CodeValidationRequested(
                    CodeValidationRequestedEvent {
                        code_id: code2_id,
                        timestamp: 1000,
                        tx_hash: H256::random(),
                    },
                )),
            ],
        }
        .setup(&db);

        service.receive_block_to_prepare(block.hash);
        assert_eq!(
            service.next().await.unwrap(),
            Event::RequestCodes([code1_id, code2_id].into())
        );

        service.receive_processed_code(code1_id);
        assert_eq!(
            service.next().await.unwrap(),
            Event::BlockPrepared(block.hash),
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn test_sub_service_start_with_codes() {
        gear_utils::init_default_logger();

        let db = Database::memory();
        let mut service = PrepareSubService::new(db.clone());

        let validated_code_id = CodeId::from([1u8; 32]);
        let requested_code_id = CodeId::from([2u8; 32]);
        let parent_block_code_id = CodeId::from([3u8; 32]);

        let code = b"1234";
        let parent_block_loaded_code_id = CodeId::generate(code);

        let chain = BlockChain::mock(1)
            .tap_mut(|chain| {
                chain.blocks[1].as_prepared_mut().codes_queue =
                    [parent_block_code_id, parent_block_loaded_code_id].into();
                chain.codes.insert(
                    parent_block_loaded_code_id,
                    CodeData {
                        original_bytes: code.to_vec(),
                        blob_info: Default::default(),
                        instrumented: None,
                    },
                );
            })
            .setup(&db);

        let block2 = chain.blocks[1].to_simple().next_block();
        let block3 = block2.next_block();

        BlockData {
            hash: block2.hash,
            header: block2.header,
            events: vec![BlockEvent::Router(RouterEvent::CodeGotValidated(
                CodeGotValidatedEvent {
                    code_id: validated_code_id,
                    valid: true,
                },
            ))],
        }
        .setup(&db);

        BlockData {
            hash: block3.hash,
            header: block3.header,
            events: vec![BlockEvent::Router(RouterEvent::CodeValidationRequested(
                CodeValidationRequestedEvent {
                    code_id: requested_code_id,
                    timestamp: 1000,
                    tx_hash: H256::random(),
                },
            ))],
        }
        .setup(&db);

        service.receive_block_to_prepare(block3.hash);
        assert_eq!(
            service.next().await.unwrap(),
            Event::RequestCodes(
                [
                    parent_block_code_id,
                    parent_block_loaded_code_id,
                    validated_code_id,
                    requested_code_id
                ]
                .into()
            )
        );

        service.receive_processed_code(validated_code_id);
        assert_eq!(
            service.next().await.unwrap(),
            Event::BlockPrepared(block3.hash),
        );
    }
}
