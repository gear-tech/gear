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

use codec::{Decode, Encode};
use core::marker::PhantomData;
use sp_runtime::{
    traits::{Bounded, One, SaturatedConversion, Saturating, Zero}
};

/// Hold bound, specifying cost of storing, expected block number for task to
/// create on it, deadlines and durations of holding.
#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq)]
pub struct HoldBound<
    BlockNumber: Saturating + Zero + Bounded + Copy + From<u64>,
    Cost: Saturating + Zero + Bounded + Ord + One + Copy + From<BlockNumber> + Into<u64>,
> {
    /// Cost of storing per block.
    cost: Cost,
    /// Expected block number task to be processed.
    expected: BlockNumber,
}

// `unused` allowed because some fns may be used in future, but clippy
// doesn't allow this due to `pub(crate)` visibility.
#[allow(unused)]
impl<
        BlockNumber: Saturating + Zero + Bounded + Copy + From<u64>,
        Cost: Saturating + Zero + Bounded + Ord + One + Copy + From<BlockNumber> + Into<u64>,
    > HoldBound<BlockNumber, Cost>
{
    /// Creates cost builder for hold bound.
    pub fn by(cost: Cost) -> HoldBoundCost<Cost, BlockNumber> {
        assert!(!cost.is_zero());
        HoldBoundCost(cost, PhantomData)
    }

    /// Returns cost of storing per block, related to current hold bound.
    pub fn cost(&self) -> Cost {
        self.cost
    }

    /// Returns expected block number task to be processed.
    pub fn expected(&self) -> BlockNumber {
        self.expected
    }

    /// Returns expected duration before task will be processed, since now.
    pub fn expected_duration(&self, current_bn: BlockNumber) -> BlockNumber {
        self.expected.saturating_sub(current_bn)
    }

    /// Returns the deadline for tasks to be processed.
    ///
    /// This deadline is exactly sum of expected block number and `reserve_for`
    /// safety duration from task pool overflow within the single block.
    pub fn deadline(&self, reserve_for: BlockNumber) -> BlockNumber {
        self.expected.saturating_add(reserve_for)
    }

    /// Returns deadline duration before task will be processed, since now.
    pub fn deadline_duration(
        &self,
        current_bn: BlockNumber,
        reserve_for: BlockNumber,
    ) -> BlockNumber {
        self.deadline(reserve_for).saturating_sub(current_bn)
    }

    /// Returns amount of gas should be locked for rent of the hold afterward.
    pub fn lock(&self, current_bn: BlockNumber, reserve_for: BlockNumber) -> Cost {
        self.deadline_duration(current_bn, reserve_for)
            .saturated_into::<Cost>()
            .saturating_mul(self.cost())
    }
}

/// Cost builder for `HoldBound<T>`.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, PartialOrd, Ord)]
pub struct HoldBoundCost<
    Cost: Saturating + Zero + Bounded + Ord + One + Copy + From<BlockNumber> + Into<u64>,
    BlockNumber: Saturating + Zero + Bounded + Copy + From<u64>,
>(Cost, PhantomData<BlockNumber>);

#[allow(unused)]
impl<
        Cost: Saturating + Zero + Bounded + Ord + One + Copy + From<BlockNumber> + Into<u64>,
        BlockNumber: Saturating + Zero + Bounded + Copy + From<u64>,
    > HoldBoundCost<Cost, BlockNumber>
{
    /// Creates bound to specific given block number.
    pub fn at(self, expected: BlockNumber) -> HoldBound<BlockNumber, Cost> {
        HoldBound {
            cost: self.0,
            expected,
        }
    }

    /// Creates bound to specific given deadline block number.
    pub fn deadline(
        self,
        deadline: BlockNumber,
        reserve_for: BlockNumber,
    ) -> HoldBound<BlockNumber, Cost> {
        let expected = deadline.saturating_sub(reserve_for);

        self.at(expected)
    }

    /// Creates bound for given duration since current block.
    pub fn duration(
        self,
        duration: BlockNumber,
        current_bn: BlockNumber,
    ) -> HoldBound<BlockNumber, Cost> {
        let expected = current_bn.saturating_add(duration);

        self.at(expected)
    }

    /// Creates maximal available bound for given gas limit.
    pub fn maximum_for(
        self,
        gas: Cost,
        current_bn: BlockNumber,
        reserve_for: BlockNumber,
    ) -> HoldBound<BlockNumber, Cost> {
        let deadline_duration = Into::<u64>::into(gas)
            .saturating_div(self.0.max(One::one()).into())
            .saturated_into::<BlockNumber>();

        let deadline = current_bn.saturating_add(deadline_duration);

        self.deadline(deadline, reserve_for)
    }

    /// Creates maximal available bound for given message id,
    /// by querying it's gas limit.
    pub fn maximum_for_message(
        self,
        gas_limit: Cost,
        current_bn: BlockNumber,
        reserve_for: BlockNumber,
    ) -> HoldBound<BlockNumber, Cost> {
        // Querying gas limit. Fails in cases of `GasTree` invalidations.
        // let gas_limit = GasHandlerOf::<T>::get_limit(message_id)
        //     .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        self.maximum_for(gas_limit, current_bn, reserve_for)
    }

    /// Zero-duration hold bound.
    pub fn zero(self, current_bn: BlockNumber) -> HoldBound<BlockNumber, Cost> {
        self.at(current_bn)
    }
}
