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
use gp::api::{
    config::GearConfig,
    generated::api::{
        runtime_types::{gear_runtime::RuntimeEvent, pallet_gear::ProcessStatus},
        storage,
    },
};
use subxt::{ext::sp_core::H256, rpc::ChainBlock};

type GearBlock = ChainBlock<GearConfig>;

impl GearApi {
    /// Return the total gas limit per block (also known as a gas budget).
    pub fn block_gas_limit(&self) -> Result<u64> {
        self.0.gas_limit().map_err(Into::into)
    }

    /// The expected average block time at which BABE should be creating blocks.
    ///
    /// Since BABE is probabilistic it is not trivial to figure out what the
    /// expected average block time should be based on the slot duration and the
    /// security parameter `c` (where `1 - c` represents the probability of a
    /// slot being empty).
    pub fn expected_block_time(&self) -> Result<u64> {
        self.0.expected_block_time().map_err(Into::into)
    }

    // Get block data
    async fn get_block_at(&self, block_hash: Option<H256>) -> Result<GearBlock> {
        Ok(self
            .0
            .rpc()
            .block(block_hash)
            .await?
            .ok_or(Error::BlockDataNotFound)?
            .block)
    }

    // Get events from the block
    async fn get_events_at(&self, block_hash: Option<H256>) -> Result<Vec<RuntimeEvent>> {
        let at = storage().system().events();

        Ok(self
            .0
            .storage()
            .fetch(&at, block_hash)
            .await?
            .ok_or(Error::StorageNotFound)?
            .into_iter()
            .map(|ev| ev.event)
            .collect())
    }

    /// Return a hash of the last block.
    pub async fn last_block_hash(&self) -> Result<H256> {
        Ok(self.get_block_at(None).await?.header.hash())
    }

    /// Return a number of the last block (also known as block height).
    pub async fn last_block_number(&self) -> Result<u32> {
        Ok(self.get_block_at(None).await?.header.number)
    }

    /// Return vector of events contained in the last block.
    pub async fn last_events(&self) -> Result<Vec<RuntimeEvent>> {
        self.get_events_at(None).await
    }

    /// Return a number of the specified block identified by the `block_hash`.
    pub async fn block_number_at(&self, block_hash: H256) -> Result<u32> {
        Ok(self.get_block_at(Some(block_hash)).await?.header.number)
    }

    /// Get a hash of a block identified by its `block_number`.
    pub async fn get_block_hash(&self, block_number: u32) -> Result<H256> {
        self.0
            .rpc()
            .block_hash(Some(block_number.into()))
            .await?
            .ok_or(Error::BlockHashNotFound)
    }

    /// Return a timestamp of the last block.
    ///
    /// The timestamp is the number of milliseconds elapsed since the Unix
    /// epoch.
    pub async fn last_block_timestamp(&self) -> Result<u64> {
        let at = storage().timestamp().now();
        self.0
            .storage()
            .fetch(&at, None)
            .await?
            .ok_or(Error::TimestampNotFound)
    }

    /// Return vector of events contained in the specified block identified by
    /// the `block_hash`.
    pub async fn events_at(&self, block_hash: H256) -> Result<Vec<RuntimeEvent>> {
        self.get_events_at(Some(block_hash)).await
    }

    /// Return vector of events contained in blocks since the block identified
    /// by the `block_hash` but no more than in `max_depth` blocks.
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

    /// Check whether the message queue processing is stopped or not.
    pub async fn queue_processing_stopped(&self) -> Result<bool> {
        let at = storage().gear().queue_state();
        self.0
            .storage()
            .fetch(&at, None)
            .await?
            .ok_or(Error::StorageNotFound)
            .map(|queue_state| matches!(queue_state, ProcessStatus::SkippedOrFailed))
    }
}
