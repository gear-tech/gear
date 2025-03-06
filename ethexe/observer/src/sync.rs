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

//! Implementation of the on-chain data synchronization.

use crate::{BlobReader, Provider, RuntimeConfig};
use alloy::rpc::types::eth::Header;
use anyhow::{anyhow, Ok, Result};
use ethexe_common::{
    db::OnChainStorage,
    events::{BlockEvent, RouterEvent},
    BlockData,
};
use ethexe_db::{BlockHeader, CodeInfo};
use futures::{
    future::{self},
    stream::FuturesUnordered,
    FutureExt,
};
use gprimitives::{CodeId, H256};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

// TODO (gsobol): make tests for ChainSync
pub(crate) struct ChainSync {
    pub provider: Provider,
    pub database: Box<dyn OnChainStorage>,
    pub blobs_reader: Arc<dyn BlobReader>,
    pub config: RuntimeConfig,
}

impl ChainSync {
    pub async fn sync(self, chain_head: Header) -> Result<(H256, Vec<(CodeId, CodeInfo)>)> {
        let block: H256 = chain_head.hash.0.into();
        let header = BlockHeader {
            height: chain_head.number as u32,
            timestamp: chain_head.timestamp,
            parent_hash: H256(chain_head.parent_hash.0),
        };

        let blocks_data = self.pre_load_data(&header).await?;

        let (chain, codes_to_load_now, codes_to_load_later) =
            self.load_chain(block, header, blocks_data).await?;

        self.load_codes(codes_to_load_now.into_iter()).await?;

        // NOTE: reverse order is important here, because by default chain was loaded in order from head to past.
        self.mark_chain_as_synced(chain.into_iter().rev()).await;

        Ok((block, codes_to_load_later))
    }

    async fn pre_load_data(&self, header: &BlockHeader) -> Result<HashMap<H256, BlockData>> {
        let Some(latest_synced_block_height) = self.database.latest_synced_block_height() else {
            log::warn!("latest_synced_block_height is not set in the database");
            return Ok(Default::default());
        };

        if header.height <= latest_synced_block_height {
            log::warn!(
                "Get a block with number {} <= latest synced block number: {}, maybe a reorg",
                header.height,
                latest_synced_block_height
            );
            // Suppose here that all data is already in db.
            return Ok(Default::default());
        }

        if (header.height - latest_synced_block_height) >= self.config.max_sync_depth {
            // TODO (gsobol): return an event to notify about too deep chain.
            return Err(anyhow!(
                    "Too much to sync: current block number: {}, Latest valid block number: {}, Max depth: {}",
                    header.height,
                    latest_synced_block_height,
                    self.config.max_sync_depth
                ));
        }

        if header.height - latest_synced_block_height < self.config.batched_sync_depth {
            // No need to pre load data, because amount of blocks is small enough.
            return Ok(Default::default());
        }

        crate::load_blocks_data_batched(
            self.provider.clone(),
            latest_synced_block_height as u64,
            header.height as u64,
            self.config.router_address,
            self.config.wvara_address,
        )
        .await
    }

    async fn load_chain(
        &self,
        block: H256,
        header: BlockHeader,
        mut blocks_data: HashMap<H256, BlockData>,
    ) -> Result<(Vec<H256>, HashSet<CodeId>, Vec<(CodeId, CodeInfo)>)> {
        let mut chain = Vec::new();
        let mut codes_to_load_now = HashSet::new();
        let mut codes_to_load_later = HashMap::new();

        let mut hash = block;
        while !self.database.block_is_synced(hash) {
            let block_data = match blocks_data.remove(&hash) {
                Some(data) => data,
                None => {
                    crate::load_block_data(
                        self.provider.clone(),
                        hash,
                        self.config.router_address,
                        self.config.wvara_address,
                        (hash == block).then_some(header.clone()),
                    )
                    .await?
                }
            };

            if hash != block_data.hash {
                unreachable!(
                    "Expected data for block hash {hash}, got for {}",
                    block_data.hash
                );
            }

            for event in &block_data.events {
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
                        self.database.set_code_info(*code_id, code_info.clone());

                        if !self.database.original_code_exists(*code_id)
                            && !codes_to_load_now.contains(code_id)
                        {
                            codes_to_load_later.insert(*code_id, code_info);
                        }
                    }
                    BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, .. }) => {
                        if codes_to_load_later.contains_key(code_id) {
                            return Err(anyhow!("Code {code_id} is validated before requested"));
                        };

                        if !self.database.original_code_exists(*code_id) {
                            codes_to_load_now.insert(*code_id);
                        }
                    }
                    _ => {}
                }
            }

            self.database.set_block_header(hash, &block_data.header);
            self.database.set_block_events(hash, &block_data.events);

            chain.push(hash);

            hash = block_data.header.parent_hash;
        }

        Ok((
            chain,
            codes_to_load_now,
            codes_to_load_later.into_iter().collect(),
        ))
    }

    async fn load_codes(&self, codes: impl Iterator<Item = CodeId>) -> Result<()> {
        // TODO (gsobol): consider to change this behaviour of loading already validated codes.
        // Must be done with ObserverService::codes_futures together.
        // May be we should use futures_bounded::FuturesMap for this.
        let codes_futures = FuturesUnordered::new();
        for code_id in codes {
            let code_info = self
                .database
                .code_info(code_id)
                .ok_or_else(|| anyhow!("Code info for code {code_id} is missing"))?;

            codes_futures.push(
                crate::read_code_from_tx_hash(
                    self.blobs_reader.clone(),
                    code_id,
                    code_info.timestamp,
                    code_info.tx_hash,
                    None,
                )
                .boxed(),
            );
        }

        for res in future::join_all(codes_futures).await {
            let (code_id, _, code) = res?;
            self.database.set_original_code(code_id, code.as_slice());
        }

        Ok(())
    }

    async fn mark_chain_as_synced(&self, chain: impl Iterator<Item = H256>) {
        for hash in chain {
            let block_header = self
                .database
                .block_header(hash)
                .unwrap_or_else(|| unreachable!("Block header for synced block {hash} is missing"));

            self.database.set_block_is_synced(hash);

            self.database
                .set_latest_synced_block_height(block_header.height);
        }
    }
}
