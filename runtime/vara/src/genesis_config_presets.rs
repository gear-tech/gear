// This file is part of Gear.

// Copyright (C) 2025 Gear Technologies Inc.
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

use super::{UNITS as TOKEN, *};
use crate::{GearBank, GearBuiltin};
use pallet_balances::GenesisConfig;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use pallet_staking::{Forcing, StakerStatus};
use runtime_primitives::AccountPublic;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{sr25519, Pair, Public};
use sp_genesis_builder::{PresetId, DEV_RUNTIME_PRESET, LOCAL_TESTNET_RUNTIME_PRESET};
use sp_runtime::traits::IdentifyAccount;
#[cfg(not(feature = "std"))]
use sp_std::alloc::format;

/// Configure initial storage state for FRAME modules.
pub fn testnet_genesis(
    initial_authorities: Vec<(
        AccountId,
        AccountId,
        BabeId,
        GrandpaId,
        ImOnlineId,
        AuthorityDiscoveryId,
    )>,
    root_key: AccountId,
    endowed_accounts: Vec<AccountId>,
    _enable_println: bool,
) -> RuntimeGenesisConfig {
    const ENDOWMENT: u128 = 1_000_000_000 * TOKEN;
    const STASH: u128 = 100 * TOKEN;
    const MIN_NOMINATOR_BOND: u128 = 50 * TOKEN;

    let _num_endowed_accounts = endowed_accounts.len();
    #[cfg(not(feature = "dev"))]
    let _root_key = root_key;

    let mut balances = endowed_accounts
        .iter()
        .map(|k: &AccountId| (k.clone(), ENDOWMENT))
        .chain(initial_authorities.iter().map(|x| (x.0.clone(), STASH)))
        .collect::<Vec<_>>();

    // Bank account.
    balances.push((GearBank::bank_address(), EXISTENTIAL_DEPOSIT));

    // Builtin accounts.
    balances.extend(
        GearBuiltin::list_builtins()
            .into_iter()
            .map(|v| (v, EXISTENTIAL_DEPOSIT)),
    );

    RuntimeGenesisConfig {
        balances: GenesisConfig { balances },
        session: SessionConfig {
            keys: initial_authorities
                .iter()
                .map(|x| {
                    (
                        x.0.clone(),
                        x.0.clone(),
                        session_keys(x.2.clone(), x.3.clone(), x.4.clone(), x.5.clone()),
                    )
                })
                .collect::<Vec<_>>(),
            ..Default::default()
        },
        staking: StakingConfig {
            validator_count: initial_authorities.len() as u32,
            minimum_validator_count: 4,
            stakers: initial_authorities
                .iter()
                .map(|x| {
                    (
                        x.0.clone(),
                        x.1.clone(),
                        STASH,
                        StakerStatus::<AccountId>::Validator,
                    )
                })
                .collect::<Vec<_>>(),
            invulnerables: initial_authorities
                .iter()
                .map(|x| x.0.clone())
                .collect::<Vec<_>>(),
            force_era: Forcing::ForceNone,
            slash_reward_fraction: Perbill::from_percent(10),
            min_nominator_bond: MIN_NOMINATOR_BOND,
            ..Default::default()
        },
        nomination_pools: NominationPoolsConfig {
            min_create_bond: 10 * ECONOMIC_UNITS,
            min_join_bond: ECONOMIC_UNITS,
            ..Default::default()
        },
        staking_rewards: StakingRewardsConfig {
            // 41.08%
            non_stakeable: Perquintill::from_rational(4_108u64, 10_000u64),
            // 85%
            ideal_stake: Perquintill::from_percent(85),
            // 5.78%
            target_inflation: Perquintill::from_rational(578u64, 10_000u64),
            ..Default::default()
        },
        babe: BabeConfig {
            epoch_config: BABE_GENESIS_EPOCH_CONFIG,
            ..Default::default()
        },
        #[cfg(feature = "dev")]
        sudo: SudoConfig {
            // Assign network admin rights.
            key: Some(root_key),
        },
        gear_bank: GearBankConfig {
            _config: Default::default(),
        },
        ..Default::default()
    }
}

/// Helper function that wraps a set of session keys.
fn session_keys(
    babe: BabeId,
    grandpa: GrandpaId,
    im_online: ImOnlineId,
    authority_discovery: AuthorityDiscoveryId,
) -> SessionKeys {
    SessionKeys {
        babe,
        grandpa,
        im_online,
        authority_discovery,
    }
}

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
    TPublic::Pair::from_string(&format!("//{seed}"), None)
        .expect("static values are valid; qed")
        .public()
}

/// Generate an account ID from seed.
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
    AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
    AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Generate authority keys.
pub fn authority_keys_from_seed(
    s: &str,
) -> (
    AccountId,
    AccountId,
    BabeId,
    GrandpaId,
    ImOnlineId,
    AuthorityDiscoveryId,
) {
    (
        get_account_id_from_seed::<sr25519::Public>(&format!("{s}//stash")),
        get_account_id_from_seed::<sr25519::Public>(s),
        get_from_seed::<BabeId>(s),
        get_from_seed::<GrandpaId>(s),
        get_from_seed::<ImOnlineId>(s),
        get_from_seed::<AuthorityDiscoveryId>(s),
    )
}

pub fn development_genesis() -> RuntimeGenesisConfig {
    testnet_genesis(
        // Initial PoA authorities
        vec![authority_keys_from_seed("Alice")],
        // Sudo account
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        // Pre-funded accounts
        vec![
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            get_account_id_from_seed::<sr25519::Public>("Bob"),
        ],
        true,
    )
}

pub fn local_testnet_genesis() -> RuntimeGenesisConfig {
    testnet_genesis(
        // Initial PoA authorities
        vec![
            authority_keys_from_seed("Alice"),
            authority_keys_from_seed("Bob"),
        ],
        // Sudo account
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        // Pre-funded accounts
        vec![
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            get_account_id_from_seed::<sr25519::Public>("Bob"),
            get_account_id_from_seed::<sr25519::Public>("Charlie"),
            get_account_id_from_seed::<sr25519::Public>("Dave"),
            get_account_id_from_seed::<sr25519::Public>("Eve"),
            get_account_id_from_seed::<sr25519::Public>("Ferdie"),
        ],
        true,
    )
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
    let patch = match id.as_str() {
        DEV_RUNTIME_PRESET => development_genesis(),
        LOCAL_TESTNET_RUNTIME_PRESET => local_testnet_genesis(),
        _ => return None,
    };

    Some(
        serde_json_wasm::to_string(&patch)
            .expect("serialization to json works.")
            .into_bytes(),
    )
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
    vec![
        PresetId::from(DEV_RUNTIME_PRESET),
        PresetId::from(LOCAL_TESTNET_RUNTIME_PRESET),
    ]
}
