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

use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage, OnChainStorage},
    events::{BlockEvent, RouterEvent},
    SimpleBlockData,
};
use ethexe_db::Database;
use ethexe_processor::{BlockProcessingResult, Processor};
use gprimitives::H256;
use std::collections::{BTreeSet, VecDeque};

#[derive(Debug, Clone)]
pub struct BlockProcessed {
    pub block_hash: H256,
}

pub(crate) struct ChainHeadProcessContext {
    pub db: Database,
    pub processor: Processor,
}

impl ChainHeadProcessContext {
    pub async fn process(mut self, head: H256) -> Result<BlockProcessed> {
        let chain = Self::collect_not_computed_blocks_chain(&self.db, head)?;

        // Bypass the chain in reverse order (from the oldest to the newest) and compute each block.
        for block_data in chain.into_iter().rev() {
            self.process_one_block(block_data).await?;
        }
        Ok(BlockProcessed { block_hash: head })
    }

    async fn process_one_block(&mut self, block_data: SimpleBlockData) -> Result<()> {
        let SimpleBlockData {
            hash: block,
            header,
        } = block_data;

        let events = OnChainStorage::block_events(&self.db, block)
            .ok_or_else(|| anyhow!("events not found for synced block {block}"))?;

        for event in &events {
            if let BlockEvent::Router(RouterEvent::CodeGotValidated {
                code_id,
                valid: true,
            }) = event
            {
                // TODO: test branch
                if !self
                    .db
                    .instrumented_code_exists(ethexe_runtime::VERSION, *code_id)
                {
                    let code = self
                        .db
                        .original_code(*code_id)
                        .ok_or(anyhow!("code not found for validated code {code_id}"))?;
                    self.processor.process_upload_code(*code_id, &code)?;
                }
            }
        }

        let parent = header.parent_hash;

        if !self.db.block_computed(parent) {
            unreachable!("Parent block {parent} must be computed before the current one {block}",);
        }

        let mut commitments_queue =
            Self::propagate_data_from_parent(&self.db, block, parent, events.iter())?;

        let block_request_events = events
            .into_iter()
            .filter_map(|event| event.to_request())
            .collect();

        let processing_result = self
            .processor
            .process_block_events(block, block_request_events)?;

        let BlockProcessingResult {
            transitions,
            states,
            schedule,
        } = processing_result;

        if !transitions.is_empty() {
            commitments_queue.push_back(block);
        }
        self.db.set_block_commitment_queue(block, commitments_queue);

        self.db.set_block_outcome(block, transitions);

        self.db.set_block_program_states(block, states);
        self.db.set_block_schedule(block, schedule);

        // Set block as valid - means state db has all states for the end of the block
        self.db.set_block_computed(block);

        self.db.set_latest_computed_block(block, header);

        Ok(())
    }

    pub fn propagate_data_from_parent<'a>(
        db: &Database,
        block: H256,
        parent: H256,
        events: impl Iterator<Item = &'a BlockEvent>,
    ) -> Result<VecDeque<H256>> {
        // Propagate prev commitment (prev not empty block hash or zero for genesis).
        if db
            .block_outcome_is_empty(parent)
            .ok_or_else(|| anyhow!("emptiness not found for computed block {parent}"))?
        {
            let parent_prev_commitment = db
                .previous_not_empty_block(parent)
                .ok_or_else(|| anyhow!("prev commitment not found for computed block {parent}"))?;
            db.set_previous_not_empty_block(block, parent_prev_commitment);
        } else {
            db.set_previous_not_empty_block(block, parent);
        }

        let mut committed_blocks_in_current = BTreeSet::new();
        let mut validated_codes_in_current = BTreeSet::new();
        let mut requested_codes_in_current = Vec::new();

        for event in events {
            match event {
                BlockEvent::Router(RouterEvent::BlockCommitted { hash }) => {
                    committed_blocks_in_current.insert(*hash);
                }
                BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, .. }) => {
                    validated_codes_in_current.insert(*code_id);
                }
                BlockEvent::Router(RouterEvent::CodeValidationRequested { code_id, .. }) => {
                    requested_codes_in_current.push(*code_id);
                }
                _ => {}
            }
        }

        // Propagate `wait for commitment` blocks queue
        let mut blocks_queue = db
            .block_commitment_queue(parent)
            .ok_or_else(|| anyhow!("commitment queue not found for computed block {parent}"))?;
        blocks_queue.retain(|hash| !committed_blocks_in_current.contains(hash));

        // Propagate `wait for code validation` blocks queue
        let mut codes_queue = db
            .block_codes_queue(parent)
            .ok_or_else(|| anyhow!("codes queue not found for computed block {parent}"))?;
        codes_queue.retain(|code_id| !validated_codes_in_current.contains(code_id));
        codes_queue.extend(requested_codes_in_current);
        db.set_block_codes_queue(block, codes_queue);

        Ok(blocks_queue)
    }

    /// Collect a chain of blocks from the head to the last not computed block.
    pub fn collect_not_computed_blocks_chain(
        db: &Database,
        head: H256,
    ) -> Result<Vec<SimpleBlockData>> {
        let mut block = head;
        let mut chain = vec![];
        while !db.block_computed(block) {
            if !db.block_is_synced(block) {
                return Err(anyhow!("Block {block} is not synced, but must be"));
            }

            let header = OnChainStorage::block_header(db, block)
                .ok_or_else(|| anyhow!("header not found for synced block {block}"))?;

            let parent = header.parent_hash;

            chain.push(SimpleBlockData {
                hash: block,
                header,
            });

            block = parent;
        }

        Ok(chain)
    }
}
