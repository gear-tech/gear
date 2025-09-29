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

use crate::{common::block_header_at_or_latest, errors};
use gearexe_common::{
    BlockHeader,
    db::{BlockMetaStorageRead, OnChainStorageRead},
    events::BlockRequestEvent,
    gear::StateTransition,
};
use gearexe_db::Database;
use gprimitives::H256;
use jsonrpsee::{
    core::{RpcResult, async_trait},
    proc_macros::rpc,
};

#[rpc(server)]
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
        block_header_at_or_latest(&self.db, hash)
    }

    async fn block_events(&self, hash: Option<H256>) -> RpcResult<Vec<BlockRequestEvent>> {
        let block_hash = block_header_at_or_latest(&self.db, hash)?.0;

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
        let block_hash = block_header_at_or_latest(&self.db, hash)?.0;

        self.db
            .block_outcome(block_hash)
            .ok_or_else(|| errors::db("Block outcome wasn't found"))?
            .into_transitions()
            .ok_or_else(|| errors::db("`block_outcome` is called on forced non-empty outcome"))
    }
}
