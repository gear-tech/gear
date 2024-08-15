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

//! Accounts storage.

use std::{cell::RefCell, collections::HashMap, fmt};

use crate::{default_users_list, Value, DEFAULT_USERS_INITIAL_BALANCE, EXISTENTIAL_DEPOSIT};
use gear_common::ProgramId;

fn init_default_accounts(storage: &mut HashMap<ProgramId, Balance>) {
    for &id in default_users_list() {
        let id = id.into();
        storage.insert(id, Balance::new(DEFAULT_USERS_INITIAL_BALANCE));
    }
}

thread_local! {
    static ACCOUNT_STORAGE: RefCell<HashMap<ProgramId, Balance>> = RefCell::new({
        let mut storage = HashMap::new();
        init_default_accounts(&mut storage);
        storage
    });
}

#[derive(Debug)]
struct Balance {
    amount: Value,
}

impl Balance {
    fn new(amount: Value) -> Self {
        Self { amount }
    }

    fn balance(&self) -> Value {
        self.amount
    }

    fn reducible_balance(&self) -> Value {
        self.amount - EXISTENTIAL_DEPOSIT
    }

    fn decrease(&mut self, amount: Value) {
        self.amount -= amount;
    }

    fn increase(&mut self, amount: Value) {
        self.amount += amount;
    }
}

pub(crate) struct Accounts;

impl Accounts {
    // Checks if account by program id exists.
    pub(crate) fn is_exist(id: ProgramId) -> bool {
        Self::balance(id) != 0
    }

    // Returns account balance.
    pub(crate) fn balance(id: ProgramId) -> Value {
        ACCOUNT_STORAGE.with_borrow(|storage| {
            storage
                .get(&id)
                .map(|balance| balance.balance())
                .unwrap_or_default()
        })
    }

    // Returns account reducible balance.
    pub(crate) fn reducible_balance(id: ProgramId) -> Value {
        ACCOUNT_STORAGE.with_borrow(|storage| {
            storage
                .get(&id)
                .map(|balance| balance.reducible_balance())
                .unwrap_or_default()
        })
    }

    // Decreases account balance.
    pub(crate) fn decrease(id: ProgramId, amount: Value, keep_alive: bool) {
        ACCOUNT_STORAGE.with_borrow_mut(|storage| {
            if let Some(balance) = storage.get_mut(&id) {
                if keep_alive && balance.reducible_balance() < amount {
                    panic!(
                        "Not enough balance to decrease, reducible: {}, value: {amount}",
                        balance.reducible_balance(),
                    );
                }
                if !keep_alive && balance.balance() < amount {
                    panic!(
                        "Not enough balance to decrease, total: {}, value: {amount}",
                        balance.balance(),
                    );
                }

                balance.decrease(amount);
                if balance.balance() < EXISTENTIAL_DEPOSIT {
                    log::debug!(
                        "Removing account {id:?} with balance {} below the existential deposit",
                        balance.balance()
                    );
                    storage.remove(&id);
                }
            }
        });
    }

    // Increases account balance.
    pub(crate) fn increase(id: ProgramId, amount: Value) {
        ACCOUNT_STORAGE.with_borrow_mut(|storage| {
            let balance = storage.entry(id).or_insert(Balance::new(0));

            if balance.balance() + amount < EXISTENTIAL_DEPOSIT {
                panic!(
                    "Failed to increase balance: the sum {} of the total balance {} \
                    and the value {} cannot be lower than the existential deposit",
                    balance.balance() + amount,
                    balance.balance(),
                    amount
                );
            }

            balance.increase(amount);
        });
    }

    // Transfers value between accounts.
    pub(crate) fn transfer(from: ProgramId, to: ProgramId, amount: Value, keep_alive: bool) {
        Self::decrease(from, amount, keep_alive);
        Self::increase(to, amount);
    }

    // Overrides account balance.
    pub(crate) fn override_balance(id: ProgramId, amount: Value) {
        if amount < EXISTENTIAL_DEPOSIT {
            panic!(
                "Failed to override balance: the amount {} cannot be lower than the existential deposit",
                amount
            );
        }

        ACCOUNT_STORAGE.with_borrow_mut(|storage| {
            storage.insert(id, Balance::new(amount));
        });
    }
}

impl fmt::Debug for Accounts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        ACCOUNT_STORAGE.with_borrow(|storage| f.debug_map().entries(storage.iter()).finish())
    }
}
