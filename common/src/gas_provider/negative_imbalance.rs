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
/// destroyed without any equal and opposite accounting.
#[derive(RuntimeDebug, PartialEq, Eq)]
pub struct NegativeImbalance<Balance: BalanceTrait>(Balance);

impl<Balance: BalanceTrait> NegativeImbalance<Balance> {
    /// Create a new negative imbalance from value amount.
    pub fn new(amount: Balance) -> Self {
        NegativeImbalance(amount)
    }

    /// Applies imbalance to some amount.
    pub fn apply_to(&self, amount: &mut Option<Balance>) {
        let new_value = amount.unwrap_or_else(Zero::zero);
        if self.0 > new_value {
            log::debug!(
                target: "essential",
                "Unaccounted gas detected: burnt {:?}, known total supply was {:?}.",
                self.0,
                *amount
            )
        }

        *amount = Some(new_value.saturating_sub(self.0));
    }
}

impl<Balance: BalanceTrait> Default for NegativeImbalance<Balance> {
    fn default() -> Self {
        Self(Zero::zero())
    }
}

impl<Balance: BalanceTrait> Imbalance for NegativeImbalance<Balance> {
    type Balance = Balance;

    fn peek(&self) -> Self::Balance {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_decreases_when_smaller_imbalance_applied() {
        let imbalance = NegativeImbalance::<u32>::new(100);
        let mut amount = Some(252);

        imbalance.apply_to(&mut amount);

        assert_eq!(amount.unwrap(), 152);
    }

    #[test]
    fn amount_drops_to_zero_when_bigger_imbalance_applied() {
        let imbalance = NegativeImbalance::<u32>::new(100);
        let mut amount = Some(42);

        imbalance.apply_to(&mut amount);

        assert_eq!(amount.unwrap(), 0);
    }
}
