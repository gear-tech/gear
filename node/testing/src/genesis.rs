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

//! Genesis Configuration.

use crate::keyring::*;
use sp_keyring::{Ed25519Keyring, Sr25519Keyring};
use sp_runtime::{Perbill, Perquintill};
use vara_runtime::{
    constants::currency::*, AccountId, BabeConfig, BalancesConfig, GenesisConfig, GrandpaConfig,
    SessionConfig, StakerStatus, StakingConfig, StakingRewardsConfig, SudoConfig, SystemConfig,
    BABE_GENESIS_EPOCH_CONFIG, WASM_BINARY,
};

fn wasm_binary() -> &'static [u8] {
    WASM_BINARY.expect(
        "Development wasm is not available. Rebuild with the `SKIP_WASM_BUILD` flag disabled.",
    )
}

/// Create genesis runtime configuration for tests.
pub fn genesis_config(code: Option<&[u8]>) -> GenesisConfig {
    config_endowed(code, Default::default())
}

/// Create genesis runtime configuration for tests adding some extra
/// endowed accounts if needed.
pub fn config_endowed(code: Option<&[u8]>, extra_endowed: Vec<AccountId>) -> GenesisConfig {
    let mut endowed = vec![
        (alice(), 111 * ECONOMIC_UNITS),
        (bob(), 100 * ECONOMIC_UNITS),
        (charlie(), 100_000_000 * ECONOMIC_UNITS),
        (dave(), 111 * ECONOMIC_UNITS),
        (eve(), 101 * ECONOMIC_UNITS),
        (ferdie(), 100 * ECONOMIC_UNITS),
    ];

    endowed.extend(
        extra_endowed
            .into_iter()
            .map(|endowed| (endowed, 100 * ECONOMIC_UNITS)),
    );

    GenesisConfig {
        system: SystemConfig {
            code: code
                .map(|x| x.to_vec())
                .unwrap_or_else(|| wasm_binary().to_vec()),
        },
        balances: BalancesConfig { balances: endowed },
        babe: BabeConfig {
            authorities: vec![],
            epoch_config: Some(BABE_GENESIS_EPOCH_CONFIG),
        },
        grandpa: GrandpaConfig {
            authorities: vec![],
        },
        session: SessionConfig {
            keys: vec![
                (
                    alice(),
                    dave(),
                    to_session_keys(&Ed25519Keyring::Alice, &Sr25519Keyring::Alice),
                ),
                (
                    bob(),
                    eve(),
                    to_session_keys(&Ed25519Keyring::Bob, &Sr25519Keyring::Bob),
                ),
                (
                    charlie(),
                    ferdie(),
                    to_session_keys(&Ed25519Keyring::Charlie, &Sr25519Keyring::Charlie),
                ),
            ],
        },
        staking: StakingConfig {
            stakers: vec![
                (
                    dave(),
                    alice(),
                    111 * ECONOMIC_UNITS,
                    StakerStatus::Validator,
                ),
                (eve(), bob(), 100 * ECONOMIC_UNITS, StakerStatus::Validator),
                (
                    ferdie(),
                    charlie(),
                    100 * ECONOMIC_UNITS,
                    StakerStatus::Validator,
                ),
            ],
            validator_count: 3,
            minimum_validator_count: 3,
            slash_reward_fraction: Perbill::from_percent(10),
            invulnerables: vec![alice(), bob(), charlie()],
            ..Default::default()
        },
        sudo: SudoConfig { key: Some(alice()) },
        im_online: Default::default(),
        authority_discovery: Default::default(),
        transaction_payment: Default::default(),
        treasury: Default::default(),
        nomination_pools: Default::default(),
        vesting: Default::default(),
        staking_rewards: StakingRewardsConfig {
            non_stakeable: Perquintill::from_rational(4108_u64, 10_000_u64), // 41.08%
            pool_balance: Default::default(),
            ideal_stake: Perquintill::from_percent(85), // 85%
            target_inflation: Perquintill::from_rational(578_u64, 10_000_u64), // 5.78%
            filtered_accounts: Default::default(),
        },
    }
}
