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

//! Gas module.

use codec::{Decode, Encode};
use scale_info::TypeInfo;

/// This trait represents a token that can be used for charging `GasCounter`.
///
/// Implementing type is expected to be super lightweight hence `Copy` (`Clone` is added
/// for consistency). If inlined there should be no observable difference compared
/// to a hand-written code.
pub trait Token: Copy + Clone {
    /// Return the amount of gas that should be taken by this token.
    ///
    /// This function should be really lightweight and must not fail. It is not
    /// expected that implementors will query the storage or do any kinds of heavy operations.
    ///
    /// That said, implementors of this function still can run into overflows
    /// while calculating the amount. In this case it is ok to use saturating operations
    /// since on overflow they will return `max_value` which should consume all gas.
    fn weight(&self) -> u64;
}

/// The result of charging gas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeResult {
    /// There was enough gas and it has been charged.
    Enough,
    /// There was not enough gas and it hasn't been charged.
    NotEnough,
}

/// Gas counter with some predefined maximum gas.
///
/// `Clone` and `Copy` traits aren't implemented for the type (however could be)
/// in order to make the data only moveable, preventing implicit/explicit copying.
#[derive(Debug, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub struct GasCounter {
    left: u64,
    burned: u64,
}

impl GasCounter {
    /// New limited gas counter with initial gas to spend.
    pub fn new(initial_amount: u64) -> Self {
        Self {
            left: initial_amount,
            burned: 0,
        }
    }

    /// Account for used gas.
    ///
    /// Returns `ChargeResult::NotEnough` if there is not enough gas or addition of the specified
    /// amount of gas has lead to overflow. On success returns `ChargeResult::Enough`.
    #[inline]
    pub fn charge(&mut self, amount: u64) -> ChargeResult {
        match self.left.checked_sub(amount) {
            None => ChargeResult::NotEnough,
            Some(new_left) => {
                self.left = new_left;
                self.burned += amount;

                ChargeResult::Enough
            }
        }
    }

    /// Account for used gas.
    ///
    /// Amount is calculated by the given `token`.
    ///
    /// Returns `ChargeResult::NotEnough` if there is not enough gas or addition of the specified
    /// amount of gas has lead to overflow. On success returns `ChargeResult::Enough`.
    #[inline]
    pub fn charge_token<Tok: Token>(&mut self, token: Tok) -> ChargeResult {
        let amount = token.weight();

        match self.left.checked_sub(amount) {
            None => ChargeResult::NotEnough,
            Some(new_left) => {
                self.left = new_left;
                self.burned += amount;

                ChargeResult::Enough
            }
        }
    }

    /// Increase gas by `amount`.
    ///
    /// Called when gas unreservation is occurred.
    // We don't decrease `burn` counter because `GasTree` manipulation is handled by separated function
    pub fn increase(&mut self, amount: u64) -> bool {
        match self.left.checked_add(amount) {
            None => false,
            Some(new_left) => {
                self.left = new_left;
                true
            }
        }
    }

    /// Reduce gas by `amount`.
    ///
    /// Called when message is sent to another program, so the gas `amount` is sent to
    /// receiving program.
    /// Or called when gas reservation is occurred.
    // In case of gas reservation:
    // We don't increase `burn` counter because `GasTree` manipulation is handled by separated function
    pub fn reduce(&mut self, amount: u64) -> ChargeResult {
        match self.left.checked_sub(amount) {
            None => ChargeResult::NotEnough,
            Some(new_left) => {
                self.left = new_left;

                ChargeResult::Enough
            }
        }
    }

    /// Refund `amount` of gas.
    // FIXME: don't use `ChargeResult`. It's semantically wrong to return it
    pub fn refund(&mut self, amount: u64) -> ChargeResult {
        if amount > u64::MAX - self.left || amount > self.burned {
            return ChargeResult::NotEnough;
        }
        match self.left.checked_add(amount) {
            None => ChargeResult::NotEnough,
            Some(new_left) => {
                self.left = new_left;
                self.burned -= amount;

                ChargeResult::Enough
            }
        }
    }

    /// Report how much gas is left.
    pub fn left(&self) -> u64 {
        self.left
    }

    /// Report how much gas is burned.
    pub fn burned(&self) -> u64 {
        self.burned
    }
}

/// Read-only representation of consumed `GasCounter`.
///
/// `Copy` trait isn't implemented for the type (however could be)
/// in order to make the data only moveable, preventing implicit/explicit copying.
#[derive(Debug, Clone)]
pub struct GasAmount {
    left: u64,
    burned: u64,
}

impl GasAmount {
    /// Report how much gas were left.
    pub fn left(&self) -> u64 {
        self.left
    }

    /// Report how much gas were burned.
    pub fn burned(&self) -> u64 {
        self.burned
    }
}

impl From<GasCounter> for GasAmount {
    fn from(gas_counter: GasCounter) -> Self {
        let GasCounter { left, burned } = gas_counter;

        Self { left, burned }
    }
}

/// Value counter with some predefined maximum value.
#[derive(Debug)]
pub struct ValueCounter(u128);

impl ValueCounter {
    /// New limited value counter with initial value to spend.
    pub fn new(initial_amount: u128) -> Self {
        Self(initial_amount)
    }

    /// Reduce value by `amount`.
    ///
    /// Called when message is sent to another program, so the value `amount` is sent to
    /// receiving program.
    pub fn reduce(&mut self, amount: u128) -> ChargeResult {
        match self.0.checked_sub(amount) {
            None => ChargeResult::NotEnough,
            Some(new_left) => {
                self.0 = new_left;

                ChargeResult::Enough
            }
        }
    }

    /// Report how much value is left.
    pub fn left(&self) -> u128 {
        self.0
    }
}

/// Gas allowance counter with some predefined maximum value.
#[derive(Debug)]
pub struct GasAllowanceCounter(u128);

impl GasAllowanceCounter {
    /// New limited gas allowance counter with initial value to spend.
    pub fn new(initial_amount: u64) -> Self {
        Self(initial_amount as u128)
    }

    /// Charge `amount` of gas.
    #[inline]
    pub fn charge(&mut self, amount: u64) -> ChargeResult {
        let amount = amount as u128;

        match self.0.checked_sub(amount) {
            None => ChargeResult::NotEnough,
            Some(new_left) => {
                self.0 = new_left;

                ChargeResult::Enough
            }
        }
    }

    /// Account for used gas.
    ///
    /// Amount is calculated by the given `token`.
    ///
    /// Returns `ChargeResult::NotEnough` if there is not enough gas or addition of the specified
    /// amount of gas has lead to overflow. On success returns `ChargeResult::Enough`.
    ///
    /// NOTE that amount is always consumed, i.e. if there is not enough gas
    /// then the counter will be set to zero.
    #[inline]
    pub fn charge_token<Tok: Token>(&mut self, token: Tok) -> ChargeResult {
        let amount = token.weight() as u128;

        match self.0.checked_sub(amount) {
            None => ChargeResult::NotEnough,
            Some(new_left) => {
                self.0 = new_left;

                ChargeResult::Enough
            }
        }
    }

    /// Refund `amount` of gas.
    pub fn refund(&mut self, amount: u64) {
        let new_value = self.0.checked_add(amount as u128);

        self.0 = new_value.unwrap_or(u128::MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::{ChargeResult, GasCounter};

    #[test]
    /// Test that `GasCounter` object returns `Enough` and decreases the remaining count
    /// on calling `charge(...)` when the remaining gas exceeds the required value,
    /// otherwise returns NotEnough
    fn limited_gas_counter_charging() {
        let mut counter = GasCounter::new(200);

        let result = counter.charge(100);

        assert_eq!(result, ChargeResult::Enough);
        assert_eq!(counter.left(), 100);

        let result = counter.charge(101);

        assert_eq!(result, ChargeResult::NotEnough);
        assert_eq!(counter.left(), 100);
    }
}
