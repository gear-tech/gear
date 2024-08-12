// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

//! Balance management.

use std::collections::HashMap;

use gear_common::{Gas, GasMultiplier, ProgramId};

use crate::{constants::Value, manager::Actors, GAS_MULTIPLIER};

/// Balance of an actor.
#[derive(Debug, Clone, Default)]
pub struct Balance {
    total: Value,
    // Primary used for ED locking for programs
    locked: Value,
}

impl Balance {
    /// Create a new balance.
    pub fn new(total: Value) -> Self {
        Self { total, locked: 0 }
    }

    /// Lock the balance.
    #[track_caller]
    pub fn set_lock(&mut self, value: Value) {
        if self.available() < value {
            panic!(
                "Trying to lock more then available balance, total: {}, lock: {}",
                self.available(),
                value
            );
        }
        self.locked = value;
    }

    /// Unlock the balance.
    pub fn empty() -> Self {
        Self {
            total: 0,
            locked: 0,
        }
    }

    /// Get the available balance.
    #[track_caller]
    pub fn available(&self) -> Value {
        self.total - self.locked
    }

    /// Get the total balance.
    pub fn total(&self) -> Value {
        self.total
    }

    /// Decrease the balance.
    ///
    /// If `respect_lock` is true, the total balance will not be decreased lower
    /// than the locked value. If `respect_lock` is false, the total balance
    /// will be decreased lower than the locked value.
    #[track_caller]
    pub fn decrease(&mut self, value: u128, respect_lock: bool) {
        if respect_lock && self.total - value < self.locked {
            panic!(
                "Not enough balance to decrease, available: {}, value: {}",
                self.available(),
                value
            );
        }

        if !respect_lock && self.total < value {
            panic!(
                "Not enough balance to decrease, total: {}, value: {}",
                self.total, value
            );
        }

        self.total -= value;
        if self.total < self.locked {
            self.locked = self.total;
        }
    }

    /// Increase the balance.
    pub fn increase(&mut self, value: Value) {
        self.total += value;
    }

    #[track_caller]
    /// Transfer the balance.
    pub(crate) fn transfer(
        actors: &mut Actors,
        from: ProgramId,
        to: ProgramId,
        value: u128,
        keep_alive: bool,
    ) {
        let mut actors = actors.borrow_mut();
        let (_, from) = actors
            .get_mut(&from)
            .unwrap_or_else(|| panic!("Sender actor id {from:?} should exist"));
        from.decrease(value, keep_alive);

        let (_, to) = actors
            .get_mut(&to)
            .unwrap_or_else(|| panic!("Receiver actor id {to:?} should exist"));
        to.increase(value);
    }
}

impl PartialEq<u128> for Balance {
    fn eq(&self, other: &Value) -> bool {
        self.total == *other
    }
}

#[derive(Default, Debug)]
struct AccountBalance {
    gas: Value,
    value: Value,
}

/// GTest bank.
#[derive(Default, Debug)]
pub struct Bank {
    accounts: HashMap<ProgramId, AccountBalance>,
}

impl Bank {
    /// Create a new bank.
    #[track_caller]
    pub fn deposit_value(
        &mut self,
        from: &mut Balance,
        to: ProgramId,
        value: Value,
        keep_alive: bool,
    ) {
        from.decrease(value, keep_alive);
        self.accounts
            .entry(to)
            .or_insert(AccountBalance { gas: 0, value: 0 })
            .value += value;
    }

    /// Deposit gas.
    #[track_caller]
    pub fn deposit_gas(&mut self, from: &mut Balance, to: ProgramId, gas: Gas, keep_alive: bool) {
        let gas_value = GAS_MULTIPLIER.gas_to_value(gas);
        from.decrease(gas_value, keep_alive);
        self.accounts
            .entry(to)
            .or_insert(AccountBalance { gas: 0, value: 0 })
            .gas += gas_value;
    }

    /// Withdraw gas.
    #[track_caller]
    pub fn spend_gas(&mut self, from: ProgramId, gas: Gas, multiplier: GasMultiplier<Value, Gas>) {
        let gas_value = multiplier.gas_to_value(gas);
        self.accounts
            .get_mut(&from)
            .unwrap_or_else(|| panic!("Bank::spend_gas: actor id {from:?} not found in bank"))
            .gas -= gas_value;
    }

    /// Withdraw value.
    #[track_caller]
    pub fn spend_gas_to(
        &mut self,
        from: ProgramId,
        to: &mut Balance,
        gas: Gas,
        multiplier: GasMultiplier<Value, Gas>,
    ) {
        self.withdraw_gas(from, to, gas, multiplier)
    }

    /// Withdraw gas.
    #[track_caller]
    pub fn withdraw_gas(
        &mut self,
        from: ProgramId,
        to: &mut Balance,
        gas_left: Gas,
        multiplier: GasMultiplier<Value, Gas>,
    ) {
        let gas_left_value = multiplier.gas_to_value(gas_left);
        self.accounts
            .get_mut(&from)
            .unwrap_or_else(|| panic!("Bank::withdraw_gas: actor id {from:?} not found in bank"))
            .gas -= gas_left_value;
        let value = multiplier.gas_to_value(gas_left);
        to.increase(value);
    }

    /// Transfer value.
    #[track_caller]
    pub fn transfer_value(&mut self, from: ProgramId, to: &mut Balance, value: Value) {
        self.accounts
            .get_mut(&from)
            .unwrap_or_else(|| panic!("Bank::transfer_value: actor id {from:?} not found in bank"))
            .value -= value;
        to.increase(value);
    }
}
