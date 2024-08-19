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

//! `gtest` bank

use std::collections::HashMap;

use gear_common::{Gas, GasMultiplier, ProgramId};

use crate::{accounts::Accounts, constants::Value, GAS_MULTIPLIER};

#[derive(Default, Debug)]
struct BankBalance {
    gas: Value,
    value: Value,
}

/// `gtest` bank.
#[derive(Default, Debug)]
pub(crate) struct Bank {
    accounts: HashMap<ProgramId, BankBalance>,
}

impl Bank {
    // Create a new bank.
    #[track_caller]
    pub(crate) fn deposit_value(&mut self, id: ProgramId, value: Value, keep_alive: bool) {
        Accounts::decrease(id, value, keep_alive);
        self.accounts
            .entry(id)
            .or_insert(BankBalance { gas: 0, value: 0 })
            .value += value;
    }

    // Deposit gas.
    #[track_caller]
    pub(crate) fn deposit_gas(&mut self, id: ProgramId, gas: Gas, keep_alive: bool) {
        let gas_value = GAS_MULTIPLIER.gas_to_value(gas);
        Accounts::decrease(id, gas_value, keep_alive);
        self.accounts
            .entry(id)
            .or_insert(BankBalance { gas: 0, value: 0 })
            .gas += gas_value;
    }

    // Withdraw gas.
    #[track_caller]
    pub(crate) fn spend_gas(
        &mut self,
        id: ProgramId,
        gas: Gas,
        multiplier: GasMultiplier<Value, Gas>,
    ) {
        let gas_value = multiplier.gas_to_value(gas);
        self.accounts
            .get_mut(&id)
            .unwrap_or_else(|| panic!("Bank::spend_gas: actor id {id:?} not found in bank"))
            .gas -= gas_value;
    }

    // Withdraw gas.
    #[track_caller]
    pub(crate) fn withdraw_gas(
        &mut self,
        id: ProgramId,
        gas_left: Gas,
        multiplier: GasMultiplier<Value, Gas>,
    ) {
        let gas_left_value = multiplier.gas_to_value(gas_left);
        self.accounts
            .get_mut(&id)
            .unwrap_or_else(|| panic!("Bank::withdraw_gas: actor id {id:?} not found in bank"))
            .gas -= gas_left_value;

        if !Accounts::can_deposit(id, gas_left_value) {
            // Unable to deposit value to account.
            // In this case unused value will be lost.
            return;
        }

        Accounts::increase(id, gas_left_value);
    }

    // Transfer value.
    #[track_caller]
    pub(crate) fn transfer_value(&mut self, from: ProgramId, to: ProgramId, value: Value) {
        self.accounts
            .get_mut(&from)
            .unwrap_or_else(|| panic!("Bank::transfer_value: actor id {from:?} not found in bank"))
            .value -= value;

        if !Accounts::can_deposit(to, value) {
            // Unable to deposit value to account.
            // In this case unused value will be lost.
            return;
        }

        Accounts::increase(to, value);
    }
}
