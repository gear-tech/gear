// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Implementation of HoldBound and HoldBound builder, specifying cost of
//! holding data.

use super::ExtManager;
use crate::{RESERVE_FOR, constants::BlockNumber};
use gear_common::{LockId, MessageId, scheduler::StorageType};
use std::cmp::Ordering;

/// Hold bound, specifying cost of storage, expected block number for task to
/// create on it, deadlines and durations of holding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoldBound {
    cost: u64,
    expected: BlockNumber,
    lock_id: Option<LockId>,
}

impl HoldBound {
    pub fn cost(&self) -> u64 {
        self.cost
    }

    pub fn expected(&self) -> BlockNumber {
        self.expected
    }

    pub fn lock_id(&self) -> Option<LockId> {
        self.lock_id
    }

    pub fn expected_duration(&self, manager: &ExtManager) -> BlockNumber {
        self.expected.saturating_sub(manager.block_height())
    }

    pub fn deadline(&self) -> BlockNumber {
        self.expected.saturating_add(RESERVE_FOR)
    }

    pub fn deadline_duration(&self, manager: &ExtManager) -> BlockNumber {
        self.deadline().saturating_sub(manager.block_height())
    }

    pub fn lock_amount(&self, manager: &ExtManager) -> u64 {
        let duration: u64 = self.deadline_duration(manager).into();
        duration.saturating_mul(self.cost())
    }
}

impl PartialOrd for HoldBound {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HoldBound {
    fn cmp(&self, other: &Self) -> Ordering {
        self.expected.cmp(&other.expected)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HoldBoundBuilder {
    storage_type: StorageType,
    cost: u64,
}

impl HoldBoundBuilder {
    pub fn new(storage_type: StorageType) -> Self {
        Self {
            storage_type,
            cost: ExtManager::cost_by_storage_type(storage_type),
        }
    }

    pub fn at(self, expected: BlockNumber) -> HoldBound {
        HoldBound {
            cost: self.cost,
            expected,
            lock_id: self.storage_type.try_into().ok(),
        }
    }

    pub fn deadline(self, deadline: BlockNumber) -> HoldBound {
        let expected = deadline.saturating_sub(RESERVE_FOR);

        self.at(expected)
    }

    pub fn duration(self, manager: &ExtManager, duration: BlockNumber) -> HoldBound {
        let expected = manager.block_height().saturating_add(duration);

        self.at(expected)
    }

    pub fn maximum_for(self, manager: &ExtManager, gas: u64) -> HoldBound {
        let deadline_duration = gas
            .saturating_div(self.cost.max(1))
            // `saturated_into` conversion: try_into + unwrap_or(MAX)
            .try_into()
            .unwrap_or(u32::MAX);
        let deadline = manager.block_height().saturating_add(deadline_duration);

        self.deadline(deadline)
    }

    pub fn maximum_for_message(self, manager: &ExtManager, message_id: MessageId) -> HoldBound {
        let gas_limit = manager.gas_tree.get_limit(message_id).unwrap_or_else(|e| {
            let err_msg = format!(
                "HoldBoundBuilder::maximum_for_message: failed getting message gas limit. \
                Message id - {message_id}. Got error - {e:?}"
            );

            unreachable!("{err_msg}");
        });

        self.maximum_for(manager, gas_limit)
    }
}
