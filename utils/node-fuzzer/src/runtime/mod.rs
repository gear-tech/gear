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

use account::*;
use block::*;
use frame_support::traits::{Currency, GenesisBuild};
use frame_system::GenesisConfig as SystemConfig;
use gear_runtime::{Balances, Runtime, SessionConfig, SessionKeys};
use pallet_balances::GenesisConfig as BalancesConfig;
use sp_io::TestExternalities;

pub use account::{account, ALICE};
pub use block::{run_to_block, run_to_next_block};

mod account;
mod block;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> TestExternalities {
    let mut t = SystemConfig::default().build_storage::<Runtime>().unwrap();

    let authorities = vec![authority_keys_from_seed("Authority")];
    // Vector of tuples of accounts and their balances
    let balances = vec![(account(ALICE), 1 << 60)];

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

    // TODO [sab] needed for the runtime fuzzer?
    SessionConfig {
        keys: authorities
            .into_iter()
            .map(|(account, babe_id, grandpa_id)| {
                (
                    account.clone(),
                    account,
                    SessionKeys {
                        babe: babe_id,
                        grandpa: grandpa_id,
                    },
                )
            })
            .collect(),
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
