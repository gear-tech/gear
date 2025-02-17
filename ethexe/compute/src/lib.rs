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
    BlockData,
};
use ethexe_db::{CodeInfo, Database};
use ethexe_observer::Query;
use ethexe_processor::{LocalOutcome, Processor};
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream};
use gprimitives::{CodeId, H256};
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub struct BlockProcessed {
    pub chain_head: H256,
    // TODO (gsobol): remove commitments, this must be handled by validator if needed
    pub commitments: Vec<BlockCommitment>,
}

#[derive(Debug, Clone)]
pub enum ComputeEvent {
    BlockProcessed(BlockProcessed),
    CodeProcessed(CodeCommitment),
}

// TODO (gsobol): add state monitoring in prometheus
// TODO (gsobol): append off-chain transactions handling
// TODO (gsobol) asap: add tests for compute service
pub struct ComputeService {
    db: Database,
    processor: Processor,
    query: Query,
    blocks_queue: VecDeque<BlockData>,
    process_block: Option<BoxFuture<'static, Result<BlockProcessed>>>,
    process_codes: JoinSet<Result<CodeCommitment>>,
}

impl Stream for ComputeService {
    type Item = Result<ComputeEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(Poll::Ready(res)) = self.process_block.as_mut().map(|f| f.poll_unpin(cx)) {
            self.process_block = self.blocks_queue.pop_front().map(|block| {
                ChainHeadProcessContext {
                    db: self.db.clone(),
                    processor: self.processor.clone(),
                    query: self.query.clone(),
                }
                .process(block)
                .boxed()
            });

            return Poll::Ready(Some(res.map(ComputeEvent::BlockProcessed)));
        }

        if let Poll::Ready(Some(res)) = self.process_codes.poll_join_next(cx) {
            return Poll::Ready(Some(
                res.map_err(Into::into)
                    .and_then(|res| res.map(ComputeEvent::CodeProcessed)),
            ));
        }

        Poll::Pending
    }
}

impl FusedStream for ComputeService {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl ComputeService {
    // TODO (gsobol): consider to create Processor inside ComputeService
    pub fn new(db: Database, processor: Processor, query: Query) -> Self {
        Self {
            db,
            processor,
            query,
            blocks_queue: VecDeque::new(),
            process_block: Default::default(),
            process_codes: Default::default(),
        }
    }

    pub fn receive_code(&mut self, code_id: CodeId, timestamp: u64, code: Vec<u8>) {
        log::info!(
            "🔢 receive a code blob, code_id {code_id}, code size {}",
            code.len()
        );

        let mut processor = self.processor.clone();
        self.process_codes.spawn_blocking(move || {
            let valid = processor.process_upload_code_raw(code_id, code.as_slice())?;
            Ok(CodeCommitment {
                id: code_id,
                timestamp,
                valid,
            })
        });
    }

    pub fn receive_chain_head(&mut self, block: BlockData) {
        log::info!(
            "📦 receive a chain head, height {}, hash {}, parent hash {}",
            block.header.height,
            block.hash,
            block.header.parent_hash
        );

        if self.process_block.is_none() {
            let context = ChainHeadProcessContext {
                db: self.db.clone(),
                processor: self.processor.clone(),
                query: self.query.clone(),
            };

            self.process_block = Some(Box::pin(context.process(block)));
        } else {
            self.blocks_queue.push_back(block);
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
                    timestamp,
                    tx_hash,
                }) => {
                    self.db
                        .set_code_info(code_id, CodeInfo { timestamp, tx_hash });
                }
                BlockRequestEvent::Router(RouterRequestEvent::ProgramCreated {
                    code_id, ..
                }) => {
                    if self.db.original_code(code_id).is_some() {
                        continue;
                    }

                    log::debug!("📥 downloading absent code: {code_id}");

                    let CodeInfo { timestamp, tx_hash } = self
                        .db
                        .code_info(code_id)
                        .ok_or_else(|| anyhow!("Code info not found for code {code_id}"))?;

                    let code = self
                        .query
                        .download_code(code_id, timestamp, tx_hash)
                        .await?;

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

    async fn process(mut self, head: BlockData) -> Result<BlockProcessed> {
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

        Ok(BlockProcessed {
            chain_head: head.hash,
            commitments,
        })
    }
}
