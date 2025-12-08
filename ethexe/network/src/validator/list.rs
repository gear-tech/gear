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

use crate::validator::ValidatorDatabase;
use anyhow::Context;
use ethexe_common::{Address, BlockHeader, ProtocolTimelines, ValidatorsVec, db::OnChainStorageRO};
use gprimitives::H256;
use std::sync::Arc;

#[derive(Debug)]
struct CurrentEra {
    index: u64,
    current_validators: ValidatorsVec,
    next_validators: Option<ValidatorsVec>,
}

/// Lightweight snapshot of [`ValidatorList`] to be used in other validator-related structures.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ValidatorListSnapshot {
    pub current_era_index: u64,
    pub current_validators: ValidatorsVec,
    pub next_validators: Option<ValidatorsVec>,
}

impl ValidatorListSnapshot {
    /// Checks if the given address is present in either the current or next era validator set.
    pub(crate) fn is_current_or_next(&self, address: Address) -> bool {
        let is_current_validator = self.current_validators.contains(&address);
        let is_next_validator = self
            .next_validators
            .as_ref()
            .is_some_and(|v| v.contains(&address));
        is_current_validator || is_next_validator
    }
}

/// Tracks current and next validator set around the latest known block.
///
/// A new [`ValidatorListSnapshot`] is produced only when the chain head moves to
/// a *strictly newer* era. Advancing within the same era (even if height
/// increases) does not emit a snapshot, which keeps downstream components from
/// reprocessing work unnecessarily.
#[derive(Debug)]
pub(crate) struct ValidatorList {
    timelines: ProtocolTimelines,
    db: Box<dyn ValidatorDatabase>,
    current_era: CurrentEra,
}

impl PartialEq<Arc<ValidatorListSnapshot>> for ValidatorList {
    fn eq(&self, other: &Arc<ValidatorListSnapshot>) -> bool {
        self.current_era.index == other.current_era_index
            && self.current_era.current_validators == other.current_validators
            && self.current_era.next_validators == other.next_validators
    }
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
            index: timelines.era_from_ts(latest_block_header.timestamp),
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
            current_era_index: self.current_era.index,
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
    ) -> anyhow::Result<Arc<ValidatorListSnapshot>> {
        let chain_head_header = self
            .db
            .block_header(chain_head)
            .context("failed to get chain head block header")?;
        let chain_head_era = self.timelines.era_from_ts(chain_head_header.timestamp);

        let current_validators = self
            .db
            .validators(chain_head_era)
            .context("validators not found")?;

        let next_validators = self.db.validators(chain_head_era + 1);

        self.current_era = CurrentEra {
            index: chain_head_era,
            current_validators,
            next_validators,
        };

        Ok(self.create_snapshot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::TryFrom;
    use ethexe_common::db::OnChainStorageRW;
    use ethexe_db::Database;

    const TIMELINES: ProtocolTimelines = ProtocolTimelines {
        genesis_ts: 0,
        era: 10,
        election: 5,
    };

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
    fn validator_list_advances() {
        let genesis_hash = H256::from_low_u64_be(0);
        let genesis_block_header = header(0, 0);
        let same_era_hash = H256::from_low_u64_be(1);
        let next_committed_validators_hash = H256::from_low_u64_be(2);
        let next_era_hash = H256::from_low_u64_be(3);

        let db = Database::memory();
        db.set_protocol_timelines(TIMELINES);
        db.set_block_header(genesis_hash, genesis_block_header);
        db.set_block_header(same_era_hash, header(1, 5));
        db.set_block_header(next_committed_validators_hash, header(2, 9));
        db.set_block_header(next_era_hash, header(3, 15));

        let current_validators = validators_vec(&[1, 2]);
        let next_validators = validators_vec(&[3, 4]);
        db.set_validators(0, current_validators.clone());

        let (mut list, init_snapshot) = ValidatorList::new(
            Box::new(db.clone()),
            genesis_block_header,
            current_validators.clone(),
        )
        .expect("init succeeds");
        assert_eq!(list.current_era.index, 0);
        assert_eq!(list.current_era.current_validators, current_validators);
        assert_eq!(list.current_era.next_validators, None);
        assert_eq!(list, init_snapshot);

        // no changes
        let snapshot = list.set_chain_head(same_era_hash).unwrap();
        assert_eq!(init_snapshot, snapshot);
        assert_eq!(list, init_snapshot);
        assert_eq!(list, snapshot);

        // next validators are known
        db.set_validators(1, next_validators.clone());

        let next_validators_snapshot = list.set_chain_head(next_committed_validators_hash).unwrap();
        assert_eq!(list.current_era.index, 0);
        assert_eq!(list.current_era.current_validators, current_validators);
        assert_eq!(
            list.current_era.next_validators,
            Some(next_validators.clone())
        );
        assert_eq!(list, next_validators_snapshot);

        // era changed
        let snapshot = list.set_chain_head(next_era_hash).unwrap();
        assert_eq!(list.current_era.index, 1);
        assert_eq!(list.current_era.current_validators, next_validators);
        assert_eq!(list.current_era.next_validators, None);
        assert_eq!(list, snapshot);

        // everything goes backwards - reorg case
        let snapshot = list.set_chain_head(genesis_hash).unwrap();
        assert_eq!(list, next_validators_snapshot);
        assert_eq!(list, snapshot);
    }
}
