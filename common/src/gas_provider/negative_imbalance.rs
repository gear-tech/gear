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
}

impl<Balance: BalanceTrait> Imbalance for NegativeImbalance<Balance> {
    type Balance = Balance;

    fn peek(&self) -> Self::Balance {
        self.0
    }

    fn apply_to(&self, amount: &mut Option<Balance>) -> Result<(), ImbalanceError> {
        let amount_value = amount.unwrap_or_else(Zero::zero);
        if let Some(amount_value) = amount_value.checked_sub(&self.0) {
            *amount = Some(amount_value);
            Ok(())
        } else {
            Err(ImbalanceError)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_decreases_when_smaller_imbalance_applied() {
        let imbalance = NegativeImbalance::<u32>::new(100);
        let mut amount = Some(252);

        let result = imbalance.apply_to(&mut amount);

        assert_eq!(Ok(()), result);
        assert_eq!(Some(152), amount);
    }

    #[test]
    fn error_returned_when_overflowing_imbalance_applied() {
        let imbalance = NegativeImbalance::<u32>::new(100);
        let mut amount = Some(42);

        let result = imbalance.apply_to(&mut amount);

        assert_eq!(Err(ImbalanceError), result);
        assert_eq!(Some(42), amount);
    }
}
