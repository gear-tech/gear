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

/// Lightweight snapshot of [`ValidatorList`] to be used in other validator-related structures.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ValidatorListSnapshot {
    pub current_era_index: u64,
    pub current_validators: ValidatorsVec,
    pub next_validators: Option<ValidatorsVec>,
}

impl ValidatorListSnapshot {
    /// Checks if the given address is present in the current era validator set.
    pub(crate) fn is_current(&self, address: Address) -> bool {
        self.current_validators.contains(&address)
    }

    /// Checks if the given address is present in the next era validator set.
    pub(crate) fn is_next(&self, address: Address) -> bool {
        self.next_validators
            .as_ref()
            .is_some_and(|v| v.contains(&address))
    }

    pub(crate) fn contains(&self, address: Address) -> bool {
        self.is_current(address) || self.is_next(address)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = Address> {
        self.current_validators.iter().copied().chain(
            self.next_validators
                .iter()
                .flat_map(|vec| vec.iter())
                .copied(),
        )
    }
}

/// Tracks current and next validator set around the latest known block.
#[derive(Debug)]
pub(crate) struct ValidatorList {
    timelines: ProtocolTimelines,
    db: Box<dyn ValidatorDatabase>,
}

impl ValidatorList {
    pub(crate) fn new(
        db: Box<dyn ValidatorDatabase>,
        timelines: ProtocolTimelines,
        latest_block_header: BlockHeader,
        latest_validators: ValidatorsVec,
    ) -> anyhow::Result<(Self, Arc<ValidatorListSnapshot>)> {
        let snapshot = ValidatorListSnapshot {
            current_era_index: timelines.era_from_ts(latest_block_header.timestamp),
            current_validators: latest_validators,
            next_validators: None,
        };
        Ok((Self { timelines, db }, Arc::new(snapshot)))
    }

    /// Refresh the current chain head and validator set snapshot.
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

        let snapshot = ValidatorListSnapshot {
            current_era_index: chain_head_era,
            current_validators,
            next_validators,
        };

        Ok(Arc::new(snapshot))
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
        slot: 1,
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
        db.set_block_header(genesis_hash, genesis_block_header);
        db.set_block_header(same_era_hash, header(1, 5));
        db.set_block_header(next_committed_validators_hash, header(2, 9));
        db.set_block_header(next_era_hash, header(3, 15));

        let current_validators = validators_vec(&[1, 2]);
        let next_validators = validators_vec(&[3, 4]);
        db.set_validators(0, current_validators.clone());

        let (mut list, init_snapshot) = ValidatorList::new(
            Box::new(db.clone()),
            TIMELINES,
            genesis_block_header,
            current_validators.clone(),
        )
        .expect("init succeeds");
        assert_eq!(init_snapshot.current_era_index, 0);
        assert_eq!(init_snapshot.current_validators, current_validators);
        assert_eq!(init_snapshot.next_validators, None);

        // no changes
        let snapshot = list.set_chain_head(same_era_hash).unwrap();
        assert_eq!(init_snapshot, snapshot);

        // next validators are known
        db.set_validators(1, next_validators.clone());

        let next_validators_snapshot = list.set_chain_head(next_committed_validators_hash).unwrap();
        assert_eq!(next_validators_snapshot.current_era_index, 0);
        assert_eq!(
            next_validators_snapshot.current_validators,
            current_validators
        );
        assert_eq!(
            next_validators_snapshot.next_validators,
            Some(next_validators.clone())
        );

        // era changed
        let snapshot = list.set_chain_head(next_era_hash).unwrap();
        assert_eq!(snapshot.current_era_index, 1);
        assert_eq!(snapshot.current_validators, next_validators);
        assert_eq!(snapshot.next_validators, None);

        // everything goes backwards - reorg case
        let snapshot = list.set_chain_head(genesis_hash).unwrap();
        assert_eq!(snapshot, next_validators_snapshot);
    }
}
