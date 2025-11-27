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
    events::{BlockEvent, RouterEvent},
};
use ethexe_db::Database;
use gprimitives::{CodeId, H256};
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
    WaitingForBlock,
    WaitingForCodes {
        codes: HashSet<CodeId>,
        not_prepared_blocks_chain: VecDeque<BlockData>,
    },
}

pub struct PrepareSubService {
    db: Database,
    state: State,
    input: VecDeque<H256>,
}

impl PrepareSubService {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            state: State::WaitingForBlock,
            input: VecDeque::new(),
        }
    }

    pub fn receive_block_to_prepare(&mut self, block: H256) {
        self.input.push_back(block);
    }

    pub fn receive_processed_code(&mut self, code_id: CodeId) {
        if let State::WaitingForCodes { codes, .. } = &mut self.state {
            codes.remove(&code_id);
        }
    }

    pub fn blocks_queue_len(&self) -> usize {
        self.input.len()
    }

    pub fn waiting_codes_count(&self) -> usize {
        if let State::WaitingForCodes { codes, .. } = &self.state {
            codes.len()
        } else {
            0
        }
    }
}

impl SubService for PrepareSubService {
    type Output = Event;

    fn poll_next(&mut self, _cx: &mut Context<'_>) -> Poll<Result<Self::Output>> {
        if let State::WaitingForBlock = &self.state {
            // Use pop_back to prepare the most recent blocks first,
            // this is the most efficient way of preparing blocks in case of multiple pending blocks.
            let Some(block_hash) = self.input.pop_back() else {
                return Poll::Pending;
            };

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
            } = missing_data(&self.db, &not_prepared_blocks_chain)?;

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

fn missing_data(db: &Database, chain: &VecDeque<BlockData>) -> Result<MissingData> {
    let mut missing_codes = HashSet::new();
    let mut missing_validated_codes = HashSet::new();

    for block in chain {
        for event in &block.events {
            match event {
                BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, .. })
                    if db.code_valid(*code_id).is_none() =>
                {
                    missing_codes.insert(*code_id);
                }
                BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, .. })
                    if db.code_valid(*code_id).is_none() =>
                {
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
        .ok_or_else(|| ComputeError::BlockValidatorsCommittedForEraNotFound(parent))?;

    for event in block.events {
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

            BlockEvent::Router(RouterEvent::ValidatorsCommittedForEra { era_index }) => {
                if era_index != latest_validators_committed_era + 1 {
                    return Err(ComputeError::ValidatorsCommitmentEraMismatch {
                        expected_era_index: latest_validators_committed_era + 1,
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
                BlockEvent::Router(RouterEvent::BatchCommitted {
                    digest: batch_committed,
                }),
                BlockEvent::Router(RouterEvent::AnnouncesCommitted(block1_announce_hash)),
                BlockEvent::Router(RouterEvent::CodeGotValidated {
                    code_id: code1_id,
                    valid: true,
                }),
                BlockEvent::Router(RouterEvent::CodeValidationRequested {
                    code_id: code2_id,
                    timestamp: 1000,
                    tx_hash: H256::random(),
                }),
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
                BlockEvent::Router(RouterEvent::CodeGotValidated {
                    code_id: code1_id,
                    valid: true,
                }),
                BlockEvent::Router(RouterEvent::CodeValidationRequested {
                    code_id: code2_id,
                    timestamp: 1000,
                    tx_hash: H256::random(),
                }),
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
}
