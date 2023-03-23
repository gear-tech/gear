// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
#[cfg(all(not(feature = "vara-native"), feature = "gear-native"))]
use gear_runtime::{
    constants::currency::*, AccountId, BabeConfig, BalancesConfig, GenesisConfig, GrandpaConfig,
    SessionConfig, SudoConfig, SystemConfig, BABE_GENESIS_EPOCH_CONFIG, WASM_BINARY,
};
use sp_keyring::{Ed25519Keyring, Sr25519Keyring};
use sp_runtime::{Perbill, Perquintill};
#[cfg(feature = "vara-native")]
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
#[cfg(feature = "vara-native")]
pub fn config_endowed(code: Option<&[u8]>, extra_endowed: Vec<AccountId>) -> GenesisConfig {
    let mut endowed = vec![
        (alice(), 111 * DOLLARS),
        (bob(), 100 * DOLLARS),
        (charlie(), 100_000_000 * DOLLARS),
        (dave(), 111 * DOLLARS),
        (eve(), 101 * DOLLARS),
        (ferdie(), 100 * DOLLARS),
    ];

    endowed.extend(
        extra_endowed
            .into_iter()
            .map(|endowed| (endowed, 100 * DOLLARS)),
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
                (dave(), alice(), 111 * DOLLARS, StakerStatus::Validator),
                (eve(), bob(), 100 * DOLLARS, StakerStatus::Validator),
                (ferdie(), charlie(), 100 * DOLLARS, StakerStatus::Validator),
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

#[cfg(all(not(feature = "vara-native"), feature = "gear-native"))]
pub fn config_endowed(code: Option<&[u8]>, extra_endowed: Vec<AccountId>) -> GenesisConfig {
    let mut endowed = vec![
        (alice(), 111 * DOLLARS),
        (bob(), 100 * DOLLARS),
        (charlie(), 100_000_000 * DOLLARS),
        (dave(), 111 * DOLLARS),
        (eve(), 101 * DOLLARS),
        (ferdie(), 100 * DOLLARS),
    ];

    endowed.extend(
        extra_endowed
            .into_iter()
            .map(|endowed| (endowed, 100 * DOLLARS)),
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
        sudo: Default::default(),
        transaction_payment: Default::default(),
    }
}
