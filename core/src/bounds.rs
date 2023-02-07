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
use sp_runtime::traits::{
    Bounded, Get, One, SaturatedConversion, Saturating, UniqueSaturatedInto, Zero,
};

/// Hold bound, specifying cost of storing, expected block number for task to
/// create on it, deadlines and durations of holding.
#[derive(Clone, Debug, Decode, Encode, Eq, PartialEq)]
pub struct HoldBound<BlockNumber: Saturating + Zero, Cost: Saturating + Zero> {
    /// Cost of storing per block.
    cost: Cost,
    /// Expected block number task to be processed.
    expected: BlockNumber,
}

// `unused` allowed because some fns may be used in future, but clippy
// doesn't allow this due to `pub(crate)` visibility.
#[allow(unused)]
impl<
        BlockNumber: Saturating + Zero + Bounded,
        Cost: Saturating + Zero + Bounded + From<BlockNumber>,
    > HoldBound<BlockNumber, Cost>
{
    /// Creates cost builder for hold bound.
    pub fn by(cost: Cost) -> HoldBoundCost<T> {
        assert!(!cost.is_zero());
        HoldBoundCost(cost)
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
    pub fn deadline(&self) -> BlockNumber {
        self.expected
            .saturating_add(CostsPerBlockOf::<T>::reserve_for())
    }

    /// Returns deadline duration before task will be processed, since now.
    pub fn deadline_duration(&self, current_bn: BlockNumber) -> BlockNumber {
        self.deadline().saturating_sub(current_bn)
    }

    /// Returns amount of gas should be locked for rent of the hold afterward.
    pub fn lock(&self, current_bn: BlockNumber) -> Cost {
        self.deadline_duration(current_bn)
            .saturated_into::<Cost>()
            .saturating_mul(self.cost())
    }
}

/// Cost builder for `HoldBound<T>`.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HoldBoundCost<
    BlockNumber: Saturating + Zero + Bounded,
    Cost: Saturating + Zero + Bounded + From<BlockNumber>,
>(Cost);

#[allow(unused)]
impl<
        BlockNumber: Saturating + Zero + Bounded,
        Cost: Saturating + Zero + Bounded + From<BlockNumber>,
    > HoldBoundCost<BlockNumber, Cost>
{
    /// Creates bound to specific given block number.
    pub fn at(self, expected: BlockNumber) -> HoldBound<T> {
        HoldBound {
            cost: self.0,
            expected,
        }
    }

    /// Creates bound to specific given deadline block number.
    pub fn deadline(self, deadline: BlockNumber) -> HoldBound<T> {
        let expected = deadline.saturating_sub(CostsPerBlockOf::<T>::reserve_for());

        self.at(expected)
    }

    /// Creates bound for given duration since current block.
    pub fn duration(self, duration: BlockNumber>) -> HoldBound<T> {
        let expected = Pallet::<T>::block_number().saturating_add(duration);

        self.at(expected)
    }

    /// Creates maximal available bound for given gas limit.
    pub fn maximum_for(self, gas: GasBalanceOf<T>) -> HoldBound<T> {
        let deadline_duration = gas
            .saturating_div(self.0.max(One::one()))
            .saturated_into::<BlockNumberFor<T>>();

        let deadline = Pallet::<T>::block_number().saturating_add(deadline_duration);

        self.deadline(deadline)
    }

    /// Creates maximal available bound for given message id,
    /// by querying it's gas limit.
    pub fn maximum_for_message(self, message_id: MessageId) -> HoldBound<T> {
        // Querying gas limit. Fails in cases of `GasTree` invalidations.
        let gas_limit = GasHandlerOf::<T>::get_limit(message_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        self.maximum_for(gas_limit)
    }

    // Zero-duration hold bound.
    pub fn zero(self) -> HoldBound<T> {
        self.at(Pallet::<T>::block_number())
    }
}
