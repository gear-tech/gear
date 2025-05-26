// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::{constants::Value, state::accounts::Accounts, GAS_MULTIPLIER};
use gear_common::{ActorId, Gas, GasMultiplier};
use std::{cell::RefCell, collections::HashMap, thread::LocalKey};

thread_local! {
    /// Bank storage.
    pub(super) static BANK_ACCOUNTS: RefCell<HashMap<ProgramId, BankBalance>> = RefCell::new(Default::default());
}

fn storage() -> &'static LocalKey<RefCell<HashMap<ProgramId, BankBalance>>> {
    if super::overlay_enabled() {
        &super::BANK_ACCOUNTS_OVERLAY
    } else {
        &BANK_ACCOUNTS
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub(super) struct BankBalance {
    pub(super) gas: Value,
    pub(super) value: Value,
}

/// `gtest` bank.
#[derive(Default, Debug)]
pub(crate) struct Bank;

impl Bank {
    // Create a new bank.
    pub(crate) fn deposit_value(&self, id: ActorId, value: Value, keep_alive: bool) {
        Accounts::decrease(id, value, keep_alive);
        storage().with_borrow_mut(|accs| {
            accs.entry(id)
                .or_insert(BankBalance { gas: 0, value: 0 })
                .value += value;
        });
    }

    // Deposit gas.
    pub(crate) fn deposit_gas(&self, id: ActorId, gas: Gas, keep_alive: bool) {
        let gas_value = GAS_MULTIPLIER.gas_to_value(gas);
        Accounts::decrease(id, gas_value, keep_alive);
        storage().with_borrow_mut(|accs| {
            accs.entry(id)
                .or_insert(BankBalance { gas: 0, value: 0 })
                .gas += gas_value;
        });
    }

    // Withdraw gas.
    pub(crate) fn spend_gas(&self, id: ActorId, gas: Gas, multiplier: GasMultiplier<Value, Gas>) {
        let gas_value = multiplier.gas_to_value(gas);
        storage().with_borrow_mut(|accs| {
            accs.get_mut(&id)
                .unwrap_or_else(|| panic!("Bank::spend_gas: actor id {id:?} not found in bank"))
                .gas -= gas_value;
        });
    }

    // Withdraw gas.
    pub(crate) fn withdraw_gas(
        &self,
        id: ActorId,
        gas_left: Gas,
        multiplier: GasMultiplier<Value, Gas>,
    ) {
        let gas_left_value = multiplier.gas_to_value(gas_left);
        storage().with_borrow_mut(|accs| {
            accs.get_mut(&id)
                .unwrap_or_else(|| panic!("Bank::spend_gas: actor id {id:?} not found in bank"))
                .gas -= gas_left_value;
        });

        if !Accounts::can_deposit(id, gas_left_value) {
            // Unable to deposit value to account.
            // In this case unused value will be lost.
            return;
        }

        Accounts::increase(id, gas_left_value);
    }

    // Transfer value.
    pub(crate) fn transfer_value(&self, from: ActorId, to: ActorId, value: Value) {
        storage().with_borrow_mut(|accs| {
            accs.get_mut(&from)
                .unwrap_or_else(|| {
                    panic!("Bank::transfer_value: actor id {from:?} not found in bank")
                })
                .value -= value;
        });

        if !Accounts::can_deposit(to, value) {
            // Unable to deposit value to account.
            // In this case unused value will be lost.
            return;
        }

        Accounts::increase(to, value);
    }

    // Transfer locked value.
    pub(crate) fn transfer_locked_value(&mut self, from: ActorId, to: ActorId, value: Value) {
        if value == 0 {
            return;
        }

        self.accounts
            .get_mut(&from)
            .unwrap_or_else(|| {
                panic!("Bank::transfer_locked_value: actor id {from:?} not found in bank")
            })
            .value -= value;

        self.accounts
            .entry(to)
            .or_insert(BankBalance { gas: 0, value: 0 })
            .value += value;
    }
}
