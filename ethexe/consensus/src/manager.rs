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

//! [`ValidatorsManager`] is responsible for providing information about validators for each block.

use anyhow::{Result, anyhow};
use ethexe_common::{ValidatorsVec, db::OnChainStorageRead, era_from_ts};
use ethexe_ethereum::router::ValidatorsProvider;
use gprimitives::H256;
use hashbrown::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct ValidatorsManager<DB> {
    db: DB,
    /// Stores mapping: first_block_era -> ValidatorsVec
    cached_validators: Arc<RwLock<HashMap<H256, ValidatorsVec>>>,
    validators_provider: Arc<dyn ValidatorsProvider + 'static>,
}

impl<DB> ValidatorsManager<DB> {
    pub fn new<V: ValidatorsProvider + 'static>(db: DB, validators_provider: V) -> Self {
        Self {
            cached_validators: Default::default(),
            db,
            validators_provider: Arc::new(validators_provider),
        }
    }
}

impl<DB> ValidatorsManager<DB>
where
    DB: OnChainStorageRead,
{
    pub async fn get_validators(self, block: H256) -> Result<ValidatorsVec> {
        let block_hint = self.find_first_block_in_era(block)?;

        if let Some(validators) = self.cached_validators.read().await.get(&block_hint) {
            return Ok(validators.clone());
        }

        // No matter query for `block` or for `block_hint` because of they are from the same era.
        let validators = self.validators_provider.validators_at(block).await?;

        self.cached_validators
            .write()
            .await
            .insert(block_hint, validators.clone());

        Ok(validators)
    }

    fn find_first_block_in_era(&self, mut block: H256) -> Result<H256> {
        let timelines = self
            .db
            .gear_exe_timelines()
            .ok_or(anyhow!("gear exe timelines not found in database"))?;

        let mut header = self
            .db
            .block_header(block)
            .ok_or(anyhow!("header not found for block: {block:?}"))?;
        let block_era = era_from_ts(header.timestamp, timelines.genesis_ts, timelines.era);

        // It is ok if we can not find parent block header. Just fetch for current block.
        while let Some(parent_header) = self.db.block_header(header.parent_hash) {
            let parent_era =
                era_from_ts(parent_header.timestamp, timelines.genesis_ts, timelines.era);

            if block_era > parent_era {
                // No need to continue because of block is first in this era
                break;
            }

            block = header.parent_hash;
            header = parent_header;
        }

        Ok(block)
    }
}
