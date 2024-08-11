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

use crate::{manager::Actors, DEFAULT_USER_ALICE, GAS_MULTIPLIER};
use gear_common::{Gas, GasMultiplier, ProgramId};
use gear_core::message::Value;

#[derive(Debug, Clone, Default)]
pub struct Balance {
    total: u128,
    // Primary used for ED locking
    locked: u128,
}

impl Balance {
    pub fn new(total: u128) -> Self {
        Self { total, locked: 0 }
    }

    #[track_caller]
    pub fn set_lock(&mut self, value: Value) {
        self.locked = value;
    }

    pub fn empty() -> Self {
        Self {
            total: 0,
            locked: 0,
        }
    }

    pub fn available(&self) -> u128 {
        self.total - self.locked
    }

    pub fn total(&self) -> u128 {
        self.total
    }

    #[track_caller]
    pub fn decrease(&mut self, value: u128, _keep_alive: bool) {
        //if self.total < value {
        //    unreachable!("Actor {:?} balance is less then sent value", from);
        //}

        self.total -= value;
    }

    pub fn increase(&mut self, value: u128) {
        self.total += value;
    }

    pub(crate) fn transfer(
        actors: &mut Actors,
        from: ProgramId,
        to: ProgramId,
        value: u128,
        keep_alive: bool,
    ) {
        let mut actors = actors.borrow_mut();
        let (_, from) = actors.get_mut(&from).expect("Actor should exist");
        from.decrease(value, keep_alive);
        let (_, to) = actors.get_mut(&to).expect("Actor should exist");
        to.increase(value);
    }
}

impl PartialEq<u128> for Balance {
    fn eq(&self, other: &u128) -> bool {
        self.total == *other
    }
}

#[derive(Default, Debug)]
struct AccountBalance {
    gas: Value,
    value: Value,
}

#[derive(Default, Debug)]
pub struct Bank {
    accounts: HashMap<ProgramId, AccountBalance>,
}

impl Bank {
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

    #[track_caller]
    pub fn deposit_gas(&mut self, from: &mut Balance, to: ProgramId, gas: Gas, keep_alive: bool) {
        let gas_value = GAS_MULTIPLIER.gas_to_value(gas);
        from.decrease(gas_value, keep_alive);
        self.accounts
            .entry(to)
            .or_insert(AccountBalance { gas: 0, value: 0 })
            .gas += gas_value;
    }

    #[track_caller]
    pub fn spend_gas(&mut self, from: ProgramId, gas: Gas, multiplier: GasMultiplier<Value, Gas>) {
        let gas_value = multiplier.gas_to_value(gas);
        self.accounts.get_mut(&from).expect("must exist").gas -= gas_value;
    }

    pub fn spend_gas_to(
        &mut self,
        from: ProgramId,
        to: &mut Balance,
        gas: Gas,
        multiplier: GasMultiplier<Value, Gas>,
    ) {
        self.withdraw_gas(from, to, gas, multiplier)
    }

    pub fn withdraw_gas(
        &mut self,
        from: ProgramId,
        to: &mut Balance,
        gas_left: Gas,
        multiplier: GasMultiplier<Value, Gas>,
    ) {
        let gas_left_value = multiplier.gas_to_value(gas_left);
        self.accounts.get_mut(&from).expect("must exist").gas -= gas_left_value;
        let value = multiplier.gas_to_value(gas_left);
        to.increase(value);
    }

    pub fn transfer_value(&mut self, from: ProgramId, to: &mut Balance, value: Value) {
        self.accounts.get_mut(&from).expect("must exist").value -= value;
        to.increase(value);
    }
}
