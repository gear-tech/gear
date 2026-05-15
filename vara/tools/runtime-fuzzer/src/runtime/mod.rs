// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use account::*;
use block::*;
use frame_support::traits::Currency;
use frame_system::GenesisConfig as SystemConfig;
use pallet_balances::{GenesisConfig as BalancesConfig, Pallet as BalancesPallet};
use runtime_primitives::Balance;
use sp_io::TestExternalities;
use sp_runtime::BuildStorage;
use vara_runtime::{
    AccountId, Balances, GearBank, Runtime, RuntimeOrigin, SessionConfig, SessionKeys,
};

pub use account::{BalanceManager, BalanceState, acc_max_balance_gas, account};
pub use block::{default_gas_limit, run_to_next_block};

mod account;
mod block;

/// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> TestExternalities {
    let mut t = SystemConfig::<Runtime>::default().build_storage().unwrap();

    // seed: "//Authority"
    let authorities = vec![authority_keys_from_seed(hex_literal::hex!(
        "bd8af67855fac56646b6d3e09bdd9b1ec3911df0d8b02c9d773dd4bdcd6f560a"
    ))];
    // Vector of tuples of accounts and their balances
    let balances = vec![
        (
            // seed: "//Alice"
            account(AccountId::from(hex_literal::hex!(
                "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
            ))),
            account::gas_to_value(account::acc_max_balance_gas()),
        ),
        (GearBank::bank_address(), Balances::minimum_balance()),
    ];

    BalancesConfig::<Runtime> {
        balances: balances
            .into_iter()
            .chain(
                authorities
                    .iter()
                    .cloned()
                    .map(|(acc, ..)| (acc, Balances::minimum_balance())),
            )
            .collect(),
    }
    .assimilate_storage(&mut t)
    .unwrap();

    // TODO #2307 needed for the runtime fuzzer?
    SessionConfig {
        keys: authorities
            .into_iter()
            .map(|(account, babe, grandpa, im_online, authority_discovery)| {
                (
                    account.clone(),
                    account,
                    SessionKeys {
                        babe,
                        grandpa,
                        im_online,
                        authority_discovery,
                    },
                )
            })
            .collect(),
        ..Default::default()
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = TestExternalities::new(t);
    ext.execute_with(|| {
        initialize(1);
        on_initialize();
    });

    ext
}

pub fn set_balance(who: AccountId, free: Balance) -> Result<(), sp_runtime::DispatchError> {
    BalancesPallet::<Runtime>::force_set_balance(RuntimeOrigin::root(), who.into(), free)
}
