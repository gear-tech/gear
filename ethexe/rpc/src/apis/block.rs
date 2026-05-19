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

#[cfg(feature = "server")]
use crate::{errors, utils};
use ethexe_common::{BlockHeader, events::BlockRequestEvent};
#[cfg(feature = "server")]
use ethexe_common::{SimpleBlockData, db::OnChainStorageRO};
#[cfg(feature = "server")]
use ethexe_db::Database;
use gprimitives::H256;
#[cfg(feature = "server")]
use jsonrpsee::core::async_trait;
use jsonrpsee::proc_macros::rpc;

#[cfg_attr(all(feature = "server", feature = "client"), rpc(server, client))]
#[cfg_attr(all(feature = "server", not(feature = "client")), rpc(server))]
#[cfg_attr(all(not(feature = "server"), feature = "client"), rpc(client))]
pub trait Block {
    #[method(name = "block_header")]
    async fn block_header(
        &self,
        hash: Option<H256>,
    ) -> jsonrpsee::core::RpcResult<(H256, BlockHeader)>;

    #[method(name = "block_events")]
    async fn block_events(
        &self,
        block_hash: Option<H256>,
    ) -> jsonrpsee::core::RpcResult<Vec<BlockRequestEvent>>;
}

#[cfg(feature = "server")]
#[derive(Clone)]
pub struct BlockApi {
    db: Database,
}

#[cfg(feature = "server")]
impl BlockApi {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[cfg(feature = "server")]
#[async_trait]
impl BlockServer for BlockApi {
    async fn block_header(
        &self,
        hash: Option<H256>,
    ) -> jsonrpsee::core::RpcResult<(H256, BlockHeader)> {
        let SimpleBlockData { hash, header } = utils::block_at_or_latest_synced(&self.db, hash)?;
        Ok((hash, header))
    }

    async fn block_events(
        &self,
        hash: Option<H256>,
    ) -> jsonrpsee::core::RpcResult<Vec<BlockRequestEvent>> {
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
}
