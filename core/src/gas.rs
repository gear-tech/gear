// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use crate::costs::RuntimeCosts;
use enum_iterator::Sequence;
use scale_info::scale::{Decode, Encode};

/// The id of the gas lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Sequence)]
#[repr(u8)]
pub enum LockId {
    /// The gas lock is provided by the mailbox.
    Mailbox,
    /// The gas lock is provided by the waitlist.
    Waitlist,
    /// The gas lock is provided by reservation.
    Reservation,
    /// The gas lock is provided by dispatch stash.
    DispatchStash,
}

/// This trait represents a token that can be used for charging `GasCounter`.
///
/// Implementing type is expected to be super lightweight hence `Copy` (`Clone` is added
/// for consistency). If inlined there should be no observable difference compared
/// to a hand-written code.
pub trait Token: Copy + Clone + Into<u64> {
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
/// `Copy` and `Clone` traits aren't implemented for the type (however could be)
/// in order to make the data only moveable, preventing explicit and implicit copying.
#[derive(Debug)]
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
    /// If there is no enough gas, then makes saturating charge and returns `NotEnough`.
    /// Else charges gas and returns `Enough`.
    pub fn charge<T: Into<u64> + Copy>(&mut self, amount: T) -> ChargeResult {
        if let Some(new_left) = self.left.checked_sub(amount.into()) {
            self.left = new_left;
            self.burned += amount.into();
            ChargeResult::Enough
        } else {
            self.burned += self.left;
            self.left = 0;
            ChargeResult::NotEnough
        }
    }

    /// Account for used gas.
    ///
    /// If there is no enough gas, then does nothing and returns `ChargeResult::NotEnough`.
    /// Else charges gas and returns `ChargeResult::Enough`.
    pub fn charge_if_enough<T: Into<u64> + Copy>(&mut self, amount: T) -> ChargeResult {
        match self.left.checked_sub(amount.into()) {
            None => ChargeResult::NotEnough,
            Some(new_left) => {
                self.left = new_left;
                self.burned += amount.into();

                ChargeResult::Enough
            }
        }
    }

    /// Increase gas by `amount`.
    ///
    /// Called when gas unreservation is occurred.
    // We don't decrease `burn` counter because `GasTree` manipulation is handled by separated function
    // TODO: uncomment when unreserving in current message features is discussed
    /*pub fn increase(&mut self, amount: u64) -> bool {
        match self.left.checked_add(amount) {
            None => false,
            Some(new_left) => {
                self.left = new_left;
                true
            }
        }
    }*/

    /// Reduce gas by `amount`.
    ///
    /// Called when message is sent to another program, so the gas `amount` is sent to
    /// receiving program.
    /// Or called when gas reservation is occurred.
    ///
    /// In case of gas reservation:
    /// We don't increase `burn` counter because `GasTree` manipulation is handled by separated function
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
    pub fn refund(&mut self, amount: u64) -> ChargeResult {
        if amount > self.burned {
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

    /// Get gas amount.
    pub fn to_amount(&self) -> GasAmount {
        GasAmount {
            left: self.left,
            burned: self.burned,
        }
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
#[derive(Clone, Debug)]
pub struct GasAllowanceCounter(u128);

impl GasAllowanceCounter {
    /// New limited gas allowance counter with initial value to spend.
    pub fn new(initial_amount: u64) -> Self {
        Self(initial_amount as u128)
    }

    /// Report how much gas allowance is left.
    pub fn left(&self) -> u64 {
        self.0 as u64
    }

    /// Account for used gas allowance.
    ///
    /// If there is no enough gas, then makes saturating charge and returns `NotEnough`.
    /// Else charges gas and returns `Enough`.
    pub fn charge<T: Into<u64>>(&mut self, amount: T) -> ChargeResult {
        if let Some(new_left) = self.0.checked_sub(Into::<u64>::into(amount) as u128) {
            self.0 = new_left;
            ChargeResult::Enough
        } else {
            self.0 = 0;
            ChargeResult::NotEnough
        }
    }

    /// Account for used gas allowance.
    ///
    /// If there is no enough gas, then does nothing and returns `ChargeResult::NotEnough`.
    /// Else charges gas and returns `ChargeResult::Enough`.
    pub fn charge_if_enough<T: Into<u64>>(&mut self, amount: T) -> ChargeResult {
        if let Some(new_left) = self.0.checked_sub(Into::<u64>::into(amount) as u128) {
            self.0 = new_left;
            ChargeResult::Enough
        } else {
            ChargeResult::NotEnough
        }
    }

    /// Refund `amount` of gas.
    pub fn refund(&mut self, amount: u64) {
        let new_value = self.0.checked_add(amount as u128);

        self.0 = new_value.unwrap_or(u128::MAX);
    }
}

/// Charging error
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display)]
pub enum ChargeError {
    /// An error occurs in attempt to charge more gas than available during execution.
    #[display(fmt = "Not enough gas to continue execution")]
    GasLimitExceeded,
    /// Gas allowance exceeded
    #[display(fmt = "Gas allowance exceeded")]
    GasAllowanceExceeded,
}

/// Counters owner can change gas limit and allowance counters.
pub trait CountersOwner {
    /// Charge for runtime api call.
    fn charge_gas_runtime(&mut self, cost: RuntimeCosts) -> Result<(), ChargeError>;
    /// Charge for runtime api call if has enough of gas, else just returns error.
    fn charge_gas_runtime_if_enough(&mut self, cost: RuntimeCosts) -> Result<(), ChargeError>;
    /// Charge gas if enough, else just returns error.
    fn charge_gas_if_enough(&mut self, amount: u64) -> Result<(), ChargeError>;
    /// Returns gas limit and gas allowance left.
    fn gas_left(&self) -> GasLeft;
    /// Set gas limit and gas allowance left.
    fn set_gas_left(&mut self, gas_left: GasLeft);
}

/// Gas limit and gas allowance left.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct GasLeft {
    /// Left gas from gas counter.
    pub gas: u64,
    /// Left gas from allowance counter.
    pub allowance: u64,
}

impl From<(u64, u64)> for GasLeft {
    fn from((gas, allowance): (u64, u64)) -> Self {
        Self { gas, allowance }
    }
}

impl From<(i64, i64)> for GasLeft {
    fn from((gas, allowance): (i64, i64)) -> Self {
        Self {
            gas: gas as u64,
            allowance: allowance as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ChargeResult, GasCounter};
    use crate::{
        costs::{HostFnWeights, RuntimeCosts},
        gas::GasAllowanceCounter,
    };

    #[test]
    /// Test that `GasCounter` object returns `Enough` and decreases the remaining count
    /// on calling `charge(...)` when the remaining gas exceeds the required value,
    /// otherwise returns NotEnough
    fn limited_gas_counter_charging() {
        let mut counter = GasCounter::new(200);

        let result = counter.charge_if_enough(100u64);

        assert_eq!(result, ChargeResult::Enough);
        assert_eq!(counter.left(), 100);

        let result = counter.charge_if_enough(101u64);

        assert_eq!(result, ChargeResult::NotEnough);
        assert_eq!(counter.left(), 100);
    }

    #[test]
    fn charge_fails() {
        let mut counter = GasCounter::new(100);
        assert_eq!(counter.charge_if_enough(200u64), ChargeResult::NotEnough);
    }

    #[test]
    fn charge_token_fails() {
        let token = RuntimeCosts::Alloc(0).token(&HostFnWeights {
            alloc: 1_000,
            ..Default::default()
        });

        let mut counter = GasCounter::new(10);
        assert_eq!(counter.charge(token), ChargeResult::NotEnough);
    }

    #[test]
    fn refund_fails() {
        let mut counter = GasCounter::new(200);
        assert_eq!(counter.charge_if_enough(100u64), ChargeResult::Enough);
        assert_eq!(counter.refund(500), ChargeResult::NotEnough);
    }

    #[test]
    fn charge_allowance_token_fails() {
        let token = RuntimeCosts::Alloc(0).token(&HostFnWeights {
            alloc: 1_000,
            ..Default::default()
        });

        let mut counter = GasAllowanceCounter::new(10);
        assert_eq!(counter.charge(token), ChargeResult::NotEnough);
    }
}
