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

//! Accounts storage.

use crate::{default_users_list, Value, DEFAULT_USERS_INITIAL_BALANCE, EXISTENTIAL_DEPOSIT};
use gear_common::{auxiliary::overlay::WithOverlay, ActorId};
use std::{collections::HashMap, fmt, thread::LocalKey};

fn init_default_accounts(storage: &mut HashMap<ActorId, Balance>) {
    for &id in default_users_list() {
        let id = id.into();
        storage.insert(id, Balance::new(DEFAULT_USERS_INITIAL_BALANCE));
    }
}

thread_local! {
    pub(super) static ACCOUNT_STORAGE: WithOverlay<HashMap<ActorId, Balance>> = WithOverlay::new({
        let mut storage = HashMap::new();
        init_default_accounts(&mut storage);
        storage
    });
}

fn storage() -> &'static LocalKey<WithOverlay<HashMap<ActorId, Balance>>> {
    &ACCOUNT_STORAGE
}

#[derive(Debug, Clone)]
pub(super) struct Balance {
    amount: Value,
}

impl Balance {
    fn new(amount: Value) -> Self {
        if amount < EXISTENTIAL_DEPOSIT {
            panic!(
                "Failed to create balance: the amount {amount} cannot be lower than the existential deposit"
            );
        }

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
    pub(crate) fn exists(id: ActorId) -> bool {
        Self::balance(id) != 0
    }

    // Returns account balance.
    pub(crate) fn balance(id: ActorId) -> Value {
        storage().with(|storage| {
            storage
                .data()
                .get(&id)
                .map(|balance| balance.balance())
                .unwrap_or_default()
        })
    }

    // Returns account reducible balance.
    pub(crate) fn reducible_balance(id: ActorId) -> Value {
        storage().with(|storage| {
            storage
                .data()
                .get(&id)
                .map(|balance| balance.reducible_balance())
                .unwrap_or_default()
        })
    }

    // Decreases account balance.
    pub(crate) fn decrease(id: ActorId, amount: Value, keep_alive: bool) {
        storage().with(|storage| {
            if let Some(balance) = storage.data_mut().get_mut(&id) {
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
                    storage.data_mut().remove(&id);
                }
            } else {
                panic!("Failed to decrease balance for account {id:?}, balance is zero");
            }
        });
    }

    // Increases account balance.
    pub(crate) fn increase(id: ActorId, amount: Value) {
        storage().with(|storage| {
            let balance = storage.data().get(&id).map(Balance::balance).unwrap_or_default();

            if balance + amount < EXISTENTIAL_DEPOSIT {
                panic!(
                    "Failed to increase balance: the sum ({}) of the total balance ({}) \
                    and the value ({}) cannot be lower than the existential deposit ({EXISTENTIAL_DEPOSIT})",
                    balance + amount,
                    balance,
                    amount
                );
            }

            storage
                .data_mut()
                .entry(id)
                .and_modify(|balance| balance.increase(amount))
                .or_insert_with(|| Balance::new(amount));
        });
    }

    // Transfers value between accounts.
    pub(crate) fn transfer(from: ActorId, to: ActorId, amount: Value, keep_alive: bool) {
        Self::decrease(from, amount, keep_alive);
        Self::increase(to, amount);
    }

    // Overrides account balance.
    pub(crate) fn override_balance(id: ActorId, amount: Value) {
        if amount < EXISTENTIAL_DEPOSIT {
            panic!(
                "Failed to override balance: the amount {amount} cannot be lower than the existential deposit"
            );
        }

        storage().with(|storage| {
            storage.data_mut().insert(id, Balance::new(amount));
        });
    }

    // Checks if value can be deposited to account.
    pub(crate) fn can_deposit(id: ActorId, amount: Value) -> bool {
        Accounts::balance(id) + amount >= EXISTENTIAL_DEPOSIT
    }

    // Clears accounts storage.
    pub(crate) fn clear() {
        storage().with(|storage| {
            storage.data_mut().clear();
            init_default_accounts(&mut storage.data_mut());
        });
    }
}

impl fmt::Debug for Accounts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        storage().with(|storage| f.debug_map().entries(storage.data().iter()).finish())
    }
}
