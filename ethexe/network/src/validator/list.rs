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
use ethexe_common::{Address, BlockHeader, ProtocolTimelines, ValidatorsVec, db::OnChainStorageRO};
use gprimitives::H256;
use std::sync::Arc;

struct CurrentEra {
    chain_head_ts: u64,
    current_validators: ValidatorsVec,
    next_validators: Option<ValidatorsVec>,
}

/// Lightweight snapshot of [`ValidatorList`] to be used in other validator-related structures.
#[derive(Debug)]
pub(crate) struct ValidatorListSnapshot {
    pub chain_head_ts: u64,
    pub timelines: ProtocolTimelines,
    pub current_validators: ValidatorsVec,
    pub next_validators: Option<ValidatorsVec>,
}

impl ValidatorListSnapshot {
    /// Returns whether `self` era index is less than `snapshot` era index.
    pub(crate) fn is_older_era(&self, snapshot: &Self) -> bool {
        let this_era = self.current_era_index();
        let snapshot_era = snapshot.current_era_index();
        this_era < snapshot_era
    }

    pub(crate) fn current_era_index(&self) -> u64 {
        self.timelines.era_from_ts(self.chain_head_ts)
    }

    pub(crate) fn block_era_index(&self, block_ts: u64) -> u64 {
        self.timelines.era_from_ts(block_ts)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = Address> {
        self.current_validators.iter().copied().chain(
            self.next_validators
                .as_deref()
                .map(|vec| vec.iter().copied())
                .into_iter()
                .flatten(),
        )
    }

    /// Checks if the given address is present in either the current or next era validator set.
    pub(crate) fn contains(&self, address: Address) -> bool {
        let is_current_validator = self.current_validators.contains(&address);
        let is_next_validator = self
            .next_validators
            .as_ref()
            .map(|v| v.contains(&address))
            .unwrap_or(false);
        is_current_validator || is_next_validator
    }
}

/// Tracks current and next validator set around the latest known block.
///
/// A new [`ValidatorListSnapshot`] is produced only when the chain head moves to
/// a *strictly newer* era. Advancing within the same era (even if height
/// increases) does not emit a snapshot, which keeps downstream components from
/// reprocessing work unnecessarily.
pub(crate) struct ValidatorList {
    timelines: ProtocolTimelines,
    db: Box<dyn ValidatorDatabase>,
    current_era: CurrentEra,
}

impl ValidatorList {
    pub(crate) fn new(
        db: Box<dyn ValidatorDatabase>,
        latest_block_header: BlockHeader,
        latest_validators: ValidatorsVec,
    ) -> anyhow::Result<(Self, Arc<ValidatorListSnapshot>)> {
        let timelines = db
            .protocol_timelines()
            .context("protocol timelines not found in db")?;
        let current_era = CurrentEra {
            chain_head_ts: latest_block_header.timestamp,
            current_validators: latest_validators,
            next_validators: None,
        };
        let this = Self {
            timelines,
            current_era,
            db,
        };
        let snapshot = this.create_snapshot();
        Ok((this, snapshot))
    }

    fn create_snapshot(&self) -> Arc<ValidatorListSnapshot> {
        let snapshot = ValidatorListSnapshot {
            chain_head_ts: self.current_era.chain_head_ts,
            timelines: self.timelines,
            current_validators: self.current_era.current_validators.clone(),
            next_validators: self.current_era.next_validators.clone(),
        };
        Arc::new(snapshot)
    }

    /// Refresh the current chain head and validator set snapshot.
    ///
    /// Returns `Some(snapshot)` only when the supplied block belongs to a later
    /// era than the current chain head. Blocks from the same or earlier era are
    /// ignored to avoid redundant validator lookups. Downstream components can
    /// use the returned snapshot to revalidate cached network messages.
    pub(crate) fn set_chain_head(
        &mut self,
        chain_head: H256,
    ) -> anyhow::Result<Option<Arc<ValidatorListSnapshot>>> {
        let chain_head_header = self
            .db
            .block_header(chain_head)
            .context("failed to get chain head block header")?;
        let chain_head_ts = chain_head_header.timestamp;

        let new_era = self.timelines.era_from_ts(chain_head_ts);
        let old_era = self.timelines.era_from_ts(self.current_era.chain_head_ts);
        if new_era <= old_era {
            return Ok(None);
        }

        let current_validators = self
            .db
            .validators(self.timelines.era_from_ts(chain_head_ts))
            .context("validators not found")?;

        self.current_era = CurrentEra {
            chain_head_ts,
            current_validators,
            next_validators: None,
        };

        Ok(Some(self.create_snapshot()))
    }

    // TODO: make actual implementation when `NextEraValidatorsCommitted` event is emitted before era transition
    #[allow(dead_code)]
    pub(crate) fn set_next_era_validators(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::TryFrom;
    use ethexe_common::db::OnChainStorageRW;
    use ethexe_db::Database;

    fn validators_vec(addresses: &[u64]) -> ValidatorsVec {
        let addrs = addresses
            .iter()
            .copied()
            .map(Address::from)
            .collect::<Vec<_>>();
        ValidatorsVec::try_from(addrs).expect("non-empty validator set")
    }

    fn header(height: u32, timestamp: u64) -> BlockHeader {
        BlockHeader {
            height,
            timestamp,
            parent_hash: H256::zero(),
        }
    }

    #[test]
    fn validator_list_advances_only_on_new_eras() {
        let timelines = ProtocolTimelines {
            genesis_ts: 0,
            era: 10,
            election: 5,
        };
        let genesis_hash = H256::from_low_u64_be(0);
        let genesis_block_header = header(0, 0);
        let same_era_hash = H256::from_low_u64_be(1);
        let next_era_hash = H256::from_low_u64_be(2);

        let db = Database::memory();
        db.set_protocol_timelines(timelines);
        db.set_block_header(genesis_hash, genesis_block_header);
        db.set_block_header(same_era_hash, header(1, 5));
        db.set_block_header(next_era_hash, header(2, 15));

        let current_validators = validators_vec(&[1, 2]);
        let next_validators = validators_vec(&[3, 4]);
        db.set_validators(0, current_validators.clone());
        db.set_validators(1, next_validators.clone());

        let (mut list, snapshot) = ValidatorList::new(
            Box::new(db.clone()),
            genesis_block_header,
            current_validators.clone(),
        )
        .expect("init succeeds");
        assert_eq!(snapshot.current_era_index(), 0);
        assert_eq!(snapshot.current_validators, current_validators);

        assert!(list.set_chain_head(same_era_hash).unwrap().is_none());
        assert_eq!(list.current_era.chain_head_ts, 0);

        let next_snapshot = list
            .set_chain_head(next_era_hash)
            .unwrap()
            .expect("new era snapshot");
        assert_eq!(next_snapshot.current_era_index(), 1);
        assert_eq!(next_snapshot.current_validators, next_validators);
        assert_eq!(list.current_era.chain_head_ts, 15);

        assert!(list.set_chain_head(genesis_hash).unwrap().is_none());
        assert_eq!(list.current_era.chain_head_ts, 15);
    }
}
