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

//! Shared compute helpers used by the Malachite-block execution path.
//!
//! Holds [`ComputeConfig`] (currently just the canonical-quarantine
//! depth) and the canonical-events utility consumed by `ethexe-processor`
//! when it folds an [`AdvanceTillEthereumBlock`](ethexe_common::mb::Transaction)
//! step into the running state.

#[derive(Debug, Clone, Copy)]
pub struct ComputeConfig {
    /// The delay in **blocks** in which events from Ethereum will be apply.
    canonical_quarantine: u8,
}

impl ComputeConfig {
    /// Constructs [`ComputeConfig`] with provided `canonical_quarantine`.
    /// In production builds `canonical_quarantine` should be equal [`ethexe_common::gear::CANONICAL_QUARANTINE`].
    pub fn new(canonical_quarantine: u8) -> Self {
        Self {
            canonical_quarantine,
        }
    }

    /// Must use only in testing purposes.
    pub fn without_quarantine() -> Self {
        Self {
            canonical_quarantine: 0,
        }
    }

    pub fn canonical_quarantine(&self) -> u8 {
        self.canonical_quarantine
    }
}

pub(crate) mod utils {
    use crate::{ComputeError, Result};
    use ethexe_common::{
        db::{ConfigStorageRO, OnChainStorageRO},
        events::BlockEvent,
    };
    use ethexe_db::Database;
    use gprimitives::H256;

    /// Finds events from Ethereum in database which can be processed in current block.
    pub fn find_canonical_events_post_quarantine(
        db: &Database,
        mut block_hash: H256,
        canonical_quarantine: u8,
    ) -> Result<Vec<BlockEvent>> {
        let genesis_block = db.config().genesis_block_hash;

        let mut block_header = db
            .block_header(block_hash)
            .ok_or_else(|| ComputeError::BlockHeaderNotFound(block_hash))?;

        for _ in 0..canonical_quarantine {
            if block_hash == genesis_block {
                return Ok(Default::default());
            }

            let parent_hash = block_header.parent_hash;
            let parent_header = db
                .block_header(parent_hash)
                .ok_or(ComputeError::BlockHeaderNotFound(parent_hash))?;

            block_hash = parent_hash;
            block_header = parent_header;
        }

        db.block_events(block_hash)
            .ok_or(ComputeError::BlockEventsNotFound(block_hash))
    }
}
