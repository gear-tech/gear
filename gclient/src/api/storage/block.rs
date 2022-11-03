// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::{GearApi, Result};
use crate::Error;
use gp::api::generated::api::runtime_types::gear_runtime::RuntimeEvent;
use subxt::{
    sp_core::H256,
    sp_runtime::{
        generic::{Block, Header},
        traits::BlakeTwo256,
        OpaqueExtrinsic,
    },
};

type GearBlock = Block<Header<u32, BlakeTwo256>, OpaqueExtrinsic>;

impl GearApi {
    pub fn block_gas_limit(&self) -> Result<u64> {
        self.0.gas_limit().map_err(Into::into)
    }

    pub fn expected_block_time(&self) -> Result<u64> {
        self.0.expected_block_time().map_err(Into::into)
    }

    async fn get_block_at(&self, block_hash: Option<H256>) -> Result<GearBlock> {
        Ok(self
            .0
            .client
            .rpc()
            .block(block_hash)
            .await?
            .ok_or(Error::BlockDataNotFound)?
            .block)
    }

    async fn get_events_at(&self, block_hash: Option<H256>) -> Result<Vec<RuntimeEvent>> {
        Ok(self
            .0
            .storage()
            .system()
            .events(block_hash)
            .await?
            .into_iter()
            .map(|ev| ev.event)
            .collect())
    }

    pub async fn last_block_hash(&self) -> Result<H256> {
        Ok(self.get_block_at(None).await?.header.hash())
    }

    pub async fn last_block_number(&self) -> Result<u32> {
        Ok(self.get_block_at(None).await?.header.number)
    }

    pub async fn last_events(&self) -> Result<Vec<RuntimeEvent>> {
        self.get_events_at(None).await
    }

    pub async fn block_number_at(&self, block_hash: H256) -> Result<u32> {
        Ok(self.get_block_at(Some(block_hash)).await?.header.number)
    }

    pub async fn events_at(&self, block_hash: H256) -> Result<Vec<RuntimeEvent>> {
        self.get_events_at(Some(block_hash)).await
    }

    pub async fn events_since(
        &self,
        block_hash: H256,
        max_depth: usize,
    ) -> Result<Vec<RuntimeEvent>> {
        let mut block_hashes = Vec::with_capacity(max_depth);

        let mut current = self.get_block_at(None).await?;
        for _ in 0..max_depth {
            let current_hash = current.header.hash();
            block_hashes.push(current_hash);

            if current_hash == block_hash {
                break;
            }

            current = self.get_block_at(Some(current.header.parent_hash)).await?;
        }

        if block_hashes.contains(&block_hash) {
            let mut events = vec![];

            for hash in block_hashes.into_iter() {
                events.append(self.events_at(hash).await?.as_mut());
            }

            Ok(events)
        } else {
            Err(Error::MaxDepthReached)
        }
    }
}
