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

//! Validator-specific networking logic that verifies signed messages
//! against on-chain state.

use crate::validator::ValidatorDatabase;
use anyhow::Context;
use ethexe_common::{
    Address, BlockHeader, ProtocolTimelines, ValidatorsVec,
    db::OnChainStorageRO,
    network::{SignedValidatorMessage, VerifiedValidatorMessage},
};
use ethexe_db::Database;
use gprimitives::H256;
use nonempty::NonEmpty;

struct ChainHead {
    header: BlockHeader,
    current_validators: ValidatorsVec,
    next_validators: Option<NonEmpty<Address>>,
}

/// Tracks validator-signed messages and admits each one once the on-chain
/// context confirms it is timely and originates from a legitimate validator.
///
/// Legitimacy is checked via the `block` attached to
/// [`ValidatorMessage`](ethexe_common::network::ValidatorMessage) and the
/// validator-signed payload it carries. The hinted era must match the current
/// chain head; eras N-1, N+2, N+3, and so on are dropped when the node is at era N.
/// Messages from era N+1 are rechecked after the next validator set arrives.
pub(crate) struct ValidatorList {
    timelines: ProtocolTimelines,
    cached_messages: CachedMessages,
    verified_messages: VecDeque<VerifiedValidatorMessage>,
    db: Box<dyn ValidatorDatabase>,
    chain_head: ChainHead,
}

impl ValidatorList {
    pub(crate) fn new(
        genesis_block_hash: H256,
        db: Box<dyn ValidatorDatabase>,
    ) -> anyhow::Result<Self> {
        let timelines = db
            .protocol_timelines()
            .context("protocol timelines not found in db")?;
        let chain_head = Self::get_chain_head(&db, &timelines, genesis_block_hash, |_| false)?
            .expect("filter is always false");
        Ok(Self {
            timelines,
            chain_head,
            db,
        })
    }

    fn get_chain_head(
        db: &impl ValidatorDatabase,
        timelines: &ProtocolTimelines,
        chain_head: H256,
        filter: F,
    ) -> anyhow::Result<ChainHead>
    where
        F: FnOnce(&BlockHeader) -> bool,
    {
        let chain_head_header = db
            .block_header(chain_head)
            .context("chain head header not found")?;

        if filter(&chain_head_header) {
            return Ok(None);
        }

        let validators = db
            .validators(timelines.era_from_ts(chain_head_header.timestamp))
            .context("validators not found")?;

        let chain_head = ChainHead {
            header: chain_head_header,
            current_validators: validators,
            next_validators: None,
        };
        Ok(Some(chain_head))
    }

    /// Refresh the current chain head and validator set snapshot.
    ///
    /// Previously cached messages are rechecked once the new context is available.
    pub(crate) fn set_chain_head(&mut self, chain_head: H256) -> anyhow::Result<bool> {
        let chain_head = Self::get_chain_head(&self.db, &self.timelines, chain_head, |chain_head_header| {
            let new_era = self.block_era_index(chain_head_header.timestamp);
            let old_era = self.current_era_index();
            new_era <= old_era
        })?;

        match chain_head {
            Some(chain_head) => {
                self.chain_head = chain_head;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub(crate) fn current_era_index(&self) -> u64 {
        self.block_era_index(self.chain_head.header.timestamp)
    }

    pub(crate) fn current_validators(&self) -> impl Iterator<Item = Address> {
        self.chain_head.current_validators.iter().copied()
    }

    pub(crate) fn contains_any_validator(&self, address: Address) -> bool {
        let is_current_validator = self.chain_head.current_validators.contains(&address);
        let is_next_validator = self
            .chain_head
            .next_validators
            .as_ref()
            .map(|v| v.contains(&address))
            .unwrap_or(false);
        is_current_validator || is_next_validator
    }

    // TODO: make actual implementation when `NextEraValidatorsCommitted` event is emitted before era transition
    #[allow(dead_code)]
    pub(crate) fn set_next_era_validators(&mut self) {}

    pub(crate) fn block_era_index(&self, block_ts: u64) -> u64 {
        (block_ts - self.genesis_timestamp) / self.era_duration
    }
}
