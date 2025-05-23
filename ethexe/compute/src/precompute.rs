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

use crate::context::ChainHeadProcessContext;
use anyhow::{anyhow, Result};
use ethexe_common::{
    db::{BlockMetaStorage, CodesStorage, OnChainStorage},
    events::{BlockEvent, RouterEvent},
    CodeInfo, SimpleBlockData,
};
use ethexe_db::Database;
use gprimitives::{CodeId, H256};
use std::{
    collections::{HashSet, VecDeque},
    pin::Pin,
    task::{Context, Poll},
};

#[derive(Debug)]
enum PreComputeState {
    WaitForBlock,
    WaitForCodes {
        // block: H256,
        chain: Vec<SimpleBlockData>,
        waiting_codes: HashSet<CodeId>,
    },
}

pub(crate) struct PreCompute {
    db: Database,

    blocks_queue: VecDeque<H256>,
    state: PreComputeState,
}

impl Future for PreCompute {
    type Output = Result<HashSet<CodeId>>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let PreComputeState::WaitForBlock = &self.state {
            if let Some(block) = self.blocks_queue.pop_back() {
                let (chain, validated_codes, codes_to_load) = self.collect_chain_codes(block)?;

                self.state = PreComputeState::WaitForCodes {
                    // block,
                    chain,
                    waiting_codes: validated_codes,
                };

                if !codes_to_load.is_empty() {
                    return Poll::Ready(Ok(codes_to_load));
                }
            }
        }

        if let PreComputeState::WaitForCodes {
            // block,
            chain,
            waiting_codes,
        } = &self.state
        {
            if waiting_codes.is_empty() {
                for block_data in chain {
                    self.db.set_block_pre_computed(block_data.hash);
                }

                self.state = PreComputeState::WaitForBlock;
            }
        }

        Poll::Pending
    }
}

impl PreCompute {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            blocks_queue: VecDeque::new(),
            state: PreComputeState::WaitForBlock,
        }
    }

    pub fn pre_compute_block(&mut self, block: H256) {
        self.blocks_queue.push_front(block);
    }

    pub fn receive_loaded_code(&mut self, code_id: CodeId) {
        if let PreComputeState::WaitForCodes { waiting_codes, .. } = &mut self.state {
            if waiting_codes.contains(&code_id) {
                waiting_codes.remove(&code_id);
            }
        }
    }

    fn collect_chain_codes(
        &self,
        block: H256,
    ) -> Result<(Vec<SimpleBlockData>, HashSet<CodeId>, HashSet<CodeId>)> {
        let chain = ChainHeadProcessContext::collect_not_computed_blocks_chain(&self.db, block)?;

        let mut validated_codes = HashSet::new();
        let mut codes_to_load = HashSet::new();
        for block in chain.iter() {
            let (block_validated_coded, block_codes_to_load) =
                self.collect_block_codes(block.hash)?;

            validated_codes.extend(block_validated_coded.into_iter());
            codes_to_load.extend(block_codes_to_load.into_iter());
        }

        Ok((chain, validated_codes, codes_to_load))
    }

    fn collect_block_codes(&self, block: H256) -> Result<(HashSet<CodeId>, HashSet<CodeId>)> {
        let events = self
            .db
            .block_events(block)
            .ok_or(anyhow!("observer must set block events"))?;

        let mut validated_codes = HashSet::new();
        let mut requested_codes = HashSet::new();

        for event in &events {
            match event {
                BlockEvent::Router(RouterEvent::CodeValidationRequested {
                    code_id,
                    timestamp,
                    tx_hash,
                }) => {
                    let code_info = CodeInfo {
                        timestamp: *timestamp,
                        tx_hash: *tx_hash,
                    };
                    self.db.set_code_blob_info(*code_id, code_info.clone());

                    if !self.db.original_code_exists(*code_id) && !validated_codes.contains(code_id)
                    {
                        requested_codes.insert(*code_id);
                    }
                }
                BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, .. }) => {
                    if requested_codes.contains(code_id) {
                        return Err(anyhow!("Code {code_id} is validated before requested"));
                    };

                    if !self.db.original_code_exists(*code_id) {
                        validated_codes.insert(*code_id);
                    }
                }
                _ => {}
            }
        }

        // Return validated codes and all codes to load
        requested_codes.extend(validated_codes.iter());
        Ok((validated_codes, requested_codes))
    }
}
