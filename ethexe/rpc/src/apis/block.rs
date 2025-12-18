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

use crate::{errors, utils};
use ethexe_common::{
    BlockHeader, SimpleBlockData,
    db::{AnnounceStorageRO, OnChainStorageRO},
    events::BlockRequestEvent,
    gear::StateTransition,
};
use ethexe_db::Database;
use gprimitives::H256;
use jsonrpsee::{
    core::{RpcResult, async_trait},
    proc_macros::rpc,
};

#[cfg_attr(not(feature = "client"), rpc(server))]
#[cfg_attr(feature = "client", rpc(server, client))]
pub trait Block {
    #[method(name = "block_header")]
    async fn block_header(&self, hash: Option<H256>) -> RpcResult<(H256, BlockHeader)>;

    #[method(name = "block_events")]
    async fn block_events(&self, block_hash: Option<H256>) -> RpcResult<Vec<BlockRequestEvent>>;

    #[method(name = "block_outcome")]
    async fn block_outcome(&self, block_hash: Option<H256>) -> RpcResult<Vec<StateTransition>>;
}

#[derive(Clone)]
pub struct BlockApi {
    db: Database,
}

impl BlockApi {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl BlockServer for BlockApi {
    async fn block_header(&self, hash: Option<H256>) -> RpcResult<(H256, BlockHeader)> {
        let SimpleBlockData { hash, header } = utils::block_at_or_latest_synced(&self.db, hash)?;
        Ok((hash, header))
    }

    async fn block_events(&self, hash: Option<H256>) -> RpcResult<Vec<BlockRequestEvent>> {
        let block_hash = utils::block_at_or_latest_synced(&self.db, hash)?.hash;

        self.db
            .block_events(block_hash)
            .map(|events| {
                events
                    .into_iter()
                    .filter_map(|event| event.to_request())
                    .collect()
            })
            .ok_or_else(|| errors::db("Block events weren't found"))
    }

    async fn block_outcome(&self, hash: Option<H256>) -> RpcResult<Vec<StateTransition>> {
        let announce_hash = utils::announce_at_or_latest_computed(&self.db, hash)?;

        self.db
            .announce_outcome(announce_hash)
            .ok_or_else(|| errors::db("Block outcome wasn't found"))
    }
}
