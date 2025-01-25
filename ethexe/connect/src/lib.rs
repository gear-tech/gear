// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Sequencer for ethexe.

use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage},
    events::{BlockEvent, BlockRequestEvent, RouterRequestEvent},
    gear::{BlockCommitment, CodeCommitment, StateTransition},
};
use ethexe_db::Database;
use ethexe_observer::{BlockData, ObserverEvent, Query};
use ethexe_processor::{LocalOutcome, Processor};
use ethexe_service_utils::AsyncFnStream;
use futures::future;
use gprimitives::H256;
use std::collections::VecDeque;
use tokio::task::JoinSet;

#[derive(Clone, Debug)]
pub enum ConnectEvent {
    BlockProcessed(Vec<BlockCommitment>),
    CodeProcessed(CodeCommitment),
}

pub struct ConnectService {
    db: Database,
    processor: Processor,
    query: Query,
    blocks_queue: VecDeque<BlockData>,
    process_block_handles: JoinSet<Result<Vec<BlockCommitment>>>,
    process_code_handles: JoinSet<Result<CodeCommitment>>,
}

impl AsyncFnStream for ConnectService {
    type Item = Result<ConnectEvent>;

    async fn like_next(&mut self) -> Option<Self::Item> {
        Some(self.next().await)
    }
}

impl ConnectService {
    pub fn new(db: Database, processor: Processor, query: Query) -> Self {
        Self {
            db,
            processor,
            query,
            blocks_queue: VecDeque::new(),
            process_block_handles: Default::default(),
            process_code_handles: Default::default(),
        }
    }

    pub fn receive_observer_event(&mut self, event: ObserverEvent) {
        match event {
            ObserverEvent::Block(block) => {
                let hash = block.hash;

                log::info!(
                    "📦 receive a chain head from observer, height {}, hash {hash}, parent hash {}",
                    block.header.height,
                    block.header.parent_hash
                );

                if self.process_block_handles.is_empty() {
                    let context = ChainHeadProcessContext {
                        db: self.db.clone(),
                        processor: self.processor.clone(),
                        query: self.query.clone(),
                    };

                    self.process_block_handles
                        .spawn(async move { context.process(block).await });
                } else {
                    self.blocks_queue.push_back(block);
                }
            }
            ObserverEvent::Blob { code_id, code } => {
                let mut processor = self.processor.clone();
                self.process_code_handles.spawn(async move {
                    let valid = processor.process_upload_code_raw(code_id, code.as_slice())?;
                    Ok(CodeCommitment { id: code_id, valid })
                });
            }
        }
    }

    pub async fn next(&mut self) -> Result<ConnectEvent> {
        tokio::select! {
            commitments = Self::next_in_join_set(&mut self.process_block_handles) => {
                commitments.map(ConnectEvent::BlockProcessed)
            }
            commitment = Self::next_in_join_set(&mut self.process_code_handles) => {
                commitment.map(ConnectEvent::CodeProcessed)
            }
        }
    }

    pub async fn next_in_join_set<T: 'static>(set: &mut JoinSet<Result<T>>) -> Result<T> {
        let res = set.join_next().await;
        if let Some(res) = res {
            match res {
                Ok(Ok(commitments)) => Ok(commitments),
                Ok(Err(err)) => Err(err),
                Err(err) => Err(err.into()),
            }
        } else {
            future::pending().await
        }
    }

    pub async fn next_processed_block(&mut self) -> Result<Vec<BlockCommitment>> {
        let res = self.process_block_handles.join_next().await;
        if let Some(res) = res {
            match res {
                Ok(Ok(commitments)) => Ok(commitments),
                Ok(Err(err)) => Err(err),
                Err(err) => Err(err.into()),
            }
        } else {
            future::pending().await
        }
    }

    pub async fn next_processed_code(&mut self) -> Result<CodeCommitment> {
        let res = self.process_code_handles.join_next().await;
        if let Some(res) = res {
            match res {
                Ok(Ok(commitments)) => Ok(commitments),
                Ok(Err(err)) => Err(err),
                Err(err) => Err(err.into()),
            }
        } else {
            future::pending().await
        }
    }
}

struct ChainHeadProcessContext {
    db: Database,
    processor: Processor,
    query: Query,
}

impl ChainHeadProcessContext {
    // TODO: remove this function.
    // This is a temporary solution to download absent codes from already processed blocks.
    async fn process_uploaded_codes_for_block(&mut self, block_hash: H256) -> Result<()> {
        let events = self.query.get_block_request_events(block_hash).await?;

        for event in events {
            match event {
                BlockRequestEvent::Router(RouterRequestEvent::CodeValidationRequested {
                    code_id,
                    blob_tx_hash,
                }) => {
                    self.db.set_code_blob_tx(code_id, blob_tx_hash);
                }
                BlockRequestEvent::Router(RouterRequestEvent::ProgramCreated {
                    code_id, ..
                }) => {
                    if self.db.original_code(code_id).is_some() {
                        continue;
                    }

                    log::debug!("📥 downloading absent code: {code_id}");

                    let blob_tx_hash = self
                        .db
                        .code_blob_tx(code_id)
                        .ok_or_else(|| anyhow!("Blob tx hash not found"))?;

                    let code = self.query.download_code(code_id, blob_tx_hash).await?;

                    self.processor
                        .process_upload_code(code_id, code.as_slice())?;
                }
                _ => continue,
            }
        }

        Ok(())
    }

    async fn process_one_block(&mut self, block_hash: H256) -> Result<Vec<StateTransition>> {
        if let Some(transitions) = self.db.block_outcome(block_hash) {
            return Ok(transitions);
        }

        self.query.propagate_meta_for_block(block_hash).await?;

        self.process_uploaded_codes_for_block(block_hash).await?;

        let block_request_events = self.query.get_block_request_events(block_hash).await?;

        let block_outcomes = self
            .processor
            .process_block_events(block_hash, block_request_events)?;

        let transition_outcomes: Vec<_> = block_outcomes
            .into_iter()
            .map(|outcome| {
                if let LocalOutcome::Transition(transition) = outcome {
                    transition
                } else {
                    unreachable!("Only transitions are expected here")
                }
            })
            .collect();

        self.db
            .set_block_is_empty(block_hash, transition_outcomes.is_empty());
        if !transition_outcomes.is_empty() {
            // Not empty blocks must be committed,
            // so append it to the `wait for commitment` queue.
            let mut queue = self
                .db
                .block_commitment_queue(block_hash)
                .ok_or_else(|| anyhow!("Commitment queue is not found for block"))?;
            queue.push_back(block_hash);
            self.db.set_block_commitment_queue(block_hash, queue);
        }

        self.db
            .set_block_outcome(block_hash, transition_outcomes.clone());

        // Set block as valid - means state db has all states for the end of the block
        self.db.set_block_end_state_is_valid(block_hash, true);

        let header = self.db.block_header(block_hash).expect("must be set; qed");
        self.db.set_latest_valid_block(block_hash, header);

        Ok(transition_outcomes)
    }

    async fn process(mut self, head: BlockData) -> Result<Vec<BlockCommitment>> {
        self.db.set_block_events(
            head.hash,
            head.events
                .into_iter()
                .flat_map(BlockEvent::to_request)
                .collect(),
        );
        self.db.set_block_header(head.hash, head.header);

        let last_committed_chain = self.query.get_last_committed_chain(head.hash).await?;

        let mut commitments = vec![];
        for block_hash in last_committed_chain.into_iter().rev() {
            let transitions = self.process_one_block(block_hash).await?;

            if transitions.is_empty() {
                // Skip empty blocks
                continue;
            }

            let header = self
                .db
                .block_header(block_hash)
                .ok_or_else(|| anyhow!("header not found, but must exist"))?;

            commitments.push(BlockCommitment {
                hash: block_hash,
                timestamp: header.timestamp,
                previous_committed_block: self
                    .db
                    .previous_committed_block(block_hash)
                    .ok_or_else(|| anyhow!("Prev commitment not found"))?,
                predecessor_block: head.hash,
                transitions,
            });
        }

        Ok(commitments)
    }
}

// pub fn receive_block_from_producer(&self, block: ProducerBlockData) {
//     let db_clone = self.db.clone();
//     tokio::spawn(async move {
//         let block_hash = block.block_hash;

//         if let Err(e) = db_clone.store_block(block).await {
//             log::error!("Failed to store block {block_hash}: {e}");
//         }
//     });
// }

// struct OffchainTransaction;

// struct ProducerBlockData {
//     block_hash: H256,
//     offchain_transactions: Vec<OffchainTransaction>,
//     process_queue_gas_allowance: Option<u64>,
// }
