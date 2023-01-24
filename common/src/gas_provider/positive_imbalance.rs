// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use super::*;

/// Opaque, move-only struct with private field to denote that value has been
/// created without any equal and opposite accounting
#[derive(RuntimeDebug, PartialEq, Eq)]
pub struct PositiveImbalance<Balance: BalanceTrait>(Balance);

impl<Balance: BalanceTrait> PositiveImbalance<Balance> {
    /// Create a new positive imbalance from value amount.
    pub fn new(amount: Balance) -> Self {
        PositiveImbalance(amount)
    }

    /// Applies imbalance to some amount.
    pub fn apply_to(&self, amount: &mut Option<Balance>) {
        let new_value = amount.unwrap_or_else(Zero::zero).saturating_add(self.0);
        *amount = Some(new_value);
    }
}

impl<Balance: BalanceTrait> Default for PositiveImbalance<Balance> {
    fn default() -> Self {
        Self(Zero::zero())
    }
}

impl<Balance: BalanceTrait> Imbalance for PositiveImbalance<Balance> {
    type Balance = Balance;

    fn peek(&self) -> Self::Balance {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_increases_when_not_overflowing_imbalance_applied() {
        let imbalance = PositiveImbalance::<u32>::new(45);
        let mut amount = Some(44);

        imbalance.apply_to(&mut amount);

        assert_eq!(amount.unwrap(), 89);
    }

    #[test]
    fn amount_increases_to_max_when_overflowing_imbalance_applied() {
        let imbalance = PositiveImbalance::<u32>::new(120);
        let mut amount = Some(u32::MAX - 100);

        imbalance.apply_to(&mut amount);

        assert_eq!(amount.unwrap(), u32::MAX);
    }
}
