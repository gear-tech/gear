// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use crate::chain_spec::{get_account_id_from_seed, get_from_seed, AccountId, Extensions};
use gear_runtime_common::{
    self,
    constants::{BANK_ADDRESS, VARA_DECIMAL, VARA_SS58PREFIX, VARA_TESTNET_TOKEN_SYMBOL},
};
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use pallet_staking::Forcing;
use sc_chain_spec::Properties;
use sc_service::ChainType;
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::sr25519;
use sp_runtime::{Perbill, Perquintill};
use vara_runtime::{
    constants::currency::{ECONOMIC_UNITS, EXISTENTIAL_DEPOSIT, UNITS as TOKEN},
    SessionKeys, StakerStatus, WASM_BINARY,
};

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<Extensions>;

/// Returns the [`Properties`] for the Vara Network Testnet (dev runtime).
pub fn vara_dev_properties() -> Properties {
    let mut p = Properties::new();

    p.insert("ss58format".into(), VARA_SS58PREFIX.into());
    p.insert("tokenDecimals".into(), VARA_DECIMAL.into());
    p.insert("tokenSymbol".into(), VARA_TESTNET_TOKEN_SYMBOL.into());

    p
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

pub fn development_config() -> Result<ChainSpec, String> {
    let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;

    Ok(ChainSpec::builder(wasm_binary, Default::default())
        .with_name("Development")
        .with_id("vara_dev")
        .with_chain_type(ChainType::Development)
        .with_genesis_config_patch(testnet_genesis(
            // Initial PoA authorities
            vec![authority_keys_from_seed("Alice")],
            // Sudo account
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            // Pre-funded accounts
            vec![
                get_account_id_from_seed::<sr25519::Public>("Alice"),
                get_account_id_from_seed::<sr25519::Public>("Bob"),
            ],
            BANK_ADDRESS.into(),
            true,
        ))
        .with_properties(vara_dev_properties())
        .build())
}

pub fn local_testnet_config() -> Result<ChainSpec, String> {
    let wasm_binary = WASM_BINARY.ok_or_else(|| "Local test wasm not available".to_string())?;

    Ok(ChainSpec::builder(wasm_binary, Default::default())
        .with_name("Vara Local Testnet")
        .with_id("vara_local_testnet")
        .with_chain_type(ChainType::Local)
        .with_genesis_config_patch(testnet_genesis(
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
            BANK_ADDRESS.into(),
            true,
        ))
        .with_properties(vara_dev_properties())
        .build())
}

/// Configure initial storage state for FRAME modules.
fn testnet_genesis(
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
    bank_account: AccountId,
    _enable_println: bool,
) -> serde_json::Value {
    const ENDOWMENT: u128 = 1_000_000 * TOKEN;
    const STASH: u128 = 100 * TOKEN;
    const MIN_NOMINATOR_BOND: u128 = 50 * TOKEN;

    let _num_endowed_accounts = endowed_accounts.len();

    let mut balances = endowed_accounts
        .iter()
        .map(|k: &AccountId| (k.clone(), ENDOWMENT))
        .chain(initial_authorities.iter().map(|x| (x.0.clone(), STASH)))
        .collect::<Vec<_>>();

    balances.push((bank_account, EXISTENTIAL_DEPOSIT));

    serde_json::json!({
        "balances": {
            "balances": balances,
        },

        "session": {
            "keys": initial_authorities
                    .iter()
                    .map(|x| {
                        (
                            x.0.clone(),
                            x.0.clone(),
                            session_keys(x.2.clone(), x.3.clone(), x.4.clone(), x.5.clone()),
                        )
                    })
                    .collect::<Vec<_>>(),
        },
        "staking": {
            "validatorCount": initial_authorities.len() as u32,
            "minimumValidatorCount": 4,
            "stakers": initial_authorities
            .iter()
            .map(|x| (x.0.clone(), x.1.clone(), STASH, StakerStatus::<AccountId>::Validator))
            .collect::<Vec<_>>(),
            "invulnerables": initial_authorities.iter().map(|x| x.0.clone()).collect::<Vec<_>>(),
            "forceEra": Forcing::ForceNone,
            "slashRewardFraction": Perbill::from_percent(10),
            "minNominatorBond": MIN_NOMINATOR_BOND,
        },
        "nominationPools": {
            "minCreateBond": 10 * ECONOMIC_UNITS,
            "minJoinBond": ECONOMIC_UNITS,
        },
        "stakingRewards": {
            "nonStakeable": Perquintill::from_rational(4108_u64, 10_000_u64), // 41.08%
            "idealStake": Perquintill::from_percent(85), // 85%
            "targetInflation": Perquintill::from_rational(578_u64, 10_000_u64), // 5.78%
        },
        "babe": {
            "epochConfig": Some(vara_runtime::BABE_GENESIS_EPOCH_CONFIG),
        },
        "sudo": {
            // Assign network admin rights.
            "key": Some(root_key),
        },
    })
}
