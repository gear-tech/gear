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

use crate::chain_spec::{get_account_id_from_seed, get_from_seed, AccountId, Extensions};
use hex_literal::hex;
use sc_service::ChainType;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_core::{crypto::UncheckedInto, sr25519};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use sp_runtime::Perbill;
use vara_runtime::{
    constants::currency::UNITS as TOKEN, BabeConfig, BalancesConfig, GearConfig, GenesisConfig,
    GrandpaConfig, SessionConfig, SessionKeys, StakerStatus, StakingConfig, SudoConfig,
    SystemConfig, WASM_BINARY,
};

// The URL for the telemetry server.
// const STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<GenesisConfig, Extensions>;

/// Generate authority keys.
pub fn authority_keys_from_seed(s: &str) -> (AccountId, AccountId, BabeId, GrandpaId) {
    (
        get_account_id_from_seed::<sr25519::Public>(&format!("{}//stash", s)),
        get_account_id_from_seed::<sr25519::Public>(s),
        get_from_seed::<BabeId>(s),
        get_from_seed::<GrandpaId>(s),
    )
}

pub fn development_config() -> Result<ChainSpec, String> {
    let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;

    Ok(ChainSpec::from_genesis(
        // Name
        "Development",
        // ID
        "vara_dev",
        ChainType::Development,
        move || {
            testnet_genesis(
                wasm_binary,
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
        },
        // Bootnodes
        vec![],
        // Telemetry
        None,
        // Protocol ID
        None,
        // Fork ID
        None,
        // Properties
        None,
        // Extensions
        Default::default(),
    ))
}

pub fn local_testnet_config() -> Result<ChainSpec, String> {
    let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;

    Ok(ChainSpec::from_genesis(
        // Name
        "Vara Local Testnet",
        // ID
        "vara_local_testnet",
        ChainType::Local,
        move || {
            testnet_genesis(
                wasm_binary,
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
        },
        // Bootnodes
        vec![],
        // Telemetry
        None,
        // Protocol ID
        None,
        // Fork ID
        None,
        // Properties
        None,
        // Extensions
        Default::default(),
    ))
}

/// Staging testnet config.
pub fn main() -> Result<ChainSpec, String> {
    let wasm_binary =
        WASM_BINARY.ok_or_else(|| "Staging testnet wasm not available".to_string())?;

    Ok(ChainSpec::from_genesis(
        "Vara Network",
        "vara_network",
        ChainType::Live,
        move || {
            testnet_genesis(
                wasm_binary,
                // Initial PoA authorities
                vec![
                    (
                        // Stash account
                        hex!["6efec345ff71786529e5e21ff50fb669a46cb052daa87fd2ce86d9ba4835a533"]
                            .into(),
                        // Controller account
                        hex!["e44eb7c78c1a46e6d7a92fcc964f5362f0fe9514b58460513f8d051ff79fa95f"]
                            .into(),
                        // 5EaEodiLKeJzuW4HACVdFLY5oZnVy9ateRqoM5CM32tfMZhW
                        hex!["6efec345ff71786529e5e21ff50fb669a46cb052daa87fd2ce86d9ba4835a533"]
                            .unchecked_into(),
                        // 5EeZhC24oePqAaYuC9KooYeG2gvWAYnH5s7tXQc9YVVGAGd3
                        hex!["724b54851966f862226e1892975858b59db7dc49f899adbeba305e5275b6c9e3"]
                            .unchecked_into(),
                    ),
                    (
                        // Stash account
                        hex!["f66b57dee7e59d9288ae6ad9d70d06b7475d01d999a29c35676d7cca3b5fbd6b"]
                            .into(),
                        // Controller account
                        hex!["3051267a473a914daab6519d363978f9102e56c0c3ef1be9bc3ae2ce37573630"]
                            .into(),
                        // 5HdoXNLoGRKGm2Cxpxh7A1vBtbehk8RrKToaQBgK4UhBdVod
                        hex!["f66b57dee7e59d9288ae6ad9d70d06b7475d01d999a29c35676d7cca3b5fbd6b"]
                            .unchecked_into(),
                        // 5FuUkGjuKVWX4RdaQ7PFBZmqVjXMJh7WbV86EKC7iWpBm7ZE
                        hex!["a9e7978e751ad81eda71e6216682674a3f6dbe0c0d0f8f12b83ebec4b7d963c5"]
                            .unchecked_into(),
                    ),
                    (
                        // Stash account
                        hex!["8ae47a881c08af1eef02292feb9cbdb9cda0e3ee127a07e1bd10f8794a45884c"]
                            .into(),
                        // Controller account
                        hex!["32ffe6532fa969364f5b900ddbd5152869a512e1616b7dab8dbfb190e4a06140"]
                            .into(),
                        // 5FCpLXt4MgbSm1hbkFpWPVwZxXN7Bz7fYexjUGPBP9XojvJo
                        hex!["8ae47a881c08af1eef02292feb9cbdb9cda0e3ee127a07e1bd10f8794a45884c"]
                            .unchecked_into(),
                        // 5EcaVDoPeacpPAFKjyiYPxt1Et7VPfjjTFBq7wu1ginzacMi
                        hex!["70c782cde31d731ebf9417c80abab1c3945e12eecfdc71adc03e2686fb3a6c1b"]
                            .unchecked_into(),
                    ),
                    (
                        // Stash account
                        hex!["96edf0641f4f4f387b15870d9610cdfc8db38c701e63b8e863e43e7ff366262b"]
                            .into(),
                        // Controller account
                        hex!["ce47cc63787a62acdf9e1d22e295fd4fccd828578ca628c9f9a67f089bf0d07e"]
                            .into(),
                        // 5FUbirpEwZtLVbz863h53XbknL7mC1kbZapGmp9PfLws7DL8
                        hex!["96edf0641f4f4f387b15870d9610cdfc8db38c701e63b8e863e43e7ff366262b"]
                            .unchecked_into(),
                        // 5Cocyk8htPZqfzcwmgybXL2wb4Z9uVpNktSXCT2nm5rAewR5
                        hex!["20bb21adf10a8909725498d447f4150a2aec5eca4adfda3321c4b9598298d8a0"]
                            .unchecked_into(),
                    ),
                    (
                        // Stash account
                        hex!["ee5941d0f4a1f50d70f27a90a655ede3f1dad5ba33a2f8fe9ea5bfe9f0d7c91e"]
                            .into(),
                        // Controller account
                        hex!["74e6f377a9181e5d458871ef42d9cc70fccf71ae92be4c2773f0e6bfdf57303b"]
                            .into(),
                        // 5HTDmXsmytGu5itJN8WvSXqGpMeD3STFTw7A2G9tbK5foEeM
                        hex!["ee5941d0f4a1f50d70f27a90a655ede3f1dad5ba33a2f8fe9ea5bfe9f0d7c91e"]
                            .unchecked_into(),
                        // 5DyCHLjKgyrr6WXEb16773Vutxu8XYxV81nAaY1aMZHUYFdb
                        hex!["5444aecf3e12dadd4e6f93ca04a7071cda2e7f90e8da7c98f55c27ab291a15f4"]
                            .unchecked_into(),
                    ),
                    (
                        // Stash account
                        hex!["32b89c4a881f873f33bd18bbcc5b9e571c43e8caa9bd6169ded16e688f0c9d65"]
                            .into(),
                        // Controller account
                        hex!["3cd2bac9ade1bc68c9e75d67c9aa9d021cb4c46ef16ba7a6ee8c1d351faa750f"]
                            .into(),
                        // 5DDD6JKfrns5WmSSsGZkSmDuc7MePzNHYVuthi8hUnfcCPjv
                        hex!["32b89c4a881f873f33bd18bbcc5b9e571c43e8caa9bd6169ded16e688f0c9d65"]
                            .unchecked_into(),
                        // 5FtffWXRfSJtfuTZqohVdQP44ajjV8ehmAArjRhKJ1mURXfC
                        hex!["a94919797c3cd522ab4de174b9bbd830020372f4c6445ba7d90b491c3547eabf"]
                            .unchecked_into(),
                    ),
                    (
                        // Stash account
                        hex!["ec84321d9751c066fb923035073a73d467d44642c457915e7496c52f45db1f65"]
                            .into(),
                        // Controller account
                        hex!["18785a9a9853652d403cfa7e89afb873c22c53e2f153c9fa5af856028de6a75f"]
                            .into(),
                        // 5FqG2TKEPQaqfQZ5hLk7auUYA9oNQeqMbDbUvMRpDZnxk4hr
                        hex!["ec84321d9751c066fb923035073a73d467d44642c457915e7496c52f45db1f65"]
                            .unchecked_into(),
                        // 5H8iTsGjMAhMqJcscNiPme75SNFmKMYByi7vzBKixik6yFqx
                        hex!["3a55ac67c147af497e9dc14debf7d5674969cc7cb2099fdf598ee6a7c36fe3b4"]
                            .unchecked_into(),
                    ),
                    (
                        // Stash account
                        hex!["f85202a9d5727171623a417147625dcd317c7ecb7ce79f8b664dfac093efda19"]
                            .into(),
                        // Controller account
                        hex!["06b0b7361b821f19c84c05a558d60a44a52d7ae350c3637b65df40baf66f4a64"]
                            .into(),
                        // 5F7YisUTjnCAxNFS8nhjmVjDp5vbA2Wt18gpLrfgJMwtzmF9
                        hex!["f85202a9d5727171623a417147625dcd317c7ecb7ce79f8b664dfac093efda19"]
                            .unchecked_into(),
                        // 5DEqMoECjfHFWLj9LxYtTjj7PiK3fFYbZ9ttxJBqP8YdcQ2U
                        hex!["e55cbde1cf31fe6b891ac4cffcce790015e77ddd0f6943653e9b4d722f72baa4"]
                            .unchecked_into(),
                    ),
                    (
                        // Stash account: 5EUtDCq49BQQG1HQkdZczrJSBynwtaksyGNcNQgiMRUbApBo
                        hex!["6ae93625c928a59f1bf9f1c01548bbd72d9bb356c56c2bb070dda79590fd4a7f"]
                            .into(),
                        // Controller account: 5FyAdXbbFzvjx4cF25uo5ziBEDUiSgucsQwtXDPUDqmLhtNM
                        hex!["acb796bd17e05ea7c1764355d3c524d8379dc88b910467379afab52776d8616a"]
                            .into(),
                        // Babe key: 5EUtDCq49BQQG1HQkdZczrJSBynwtaksyGNcNQgiMRUbApBo
                        hex!["6ae93625c928a59f1bf9f1c01548bbd72d9bb356c56c2bb070dda79590fd4a7f"]
                            .unchecked_into(),
                        // Grandpa key: 5D6XHQGDEhmB2296VXza6m2gBfPvgpbbuAaHJsC72nZX5LdK
                        hex!["2d9f2166122f449c2dcb92d4de97cca7043158968d82e27bacade4015ec55b00"]
                            .unchecked_into(),
                    ),
                    (
                        // Stash account: 5HTmWmJRVCAaC8unVkjDcQMWhACpiA85ymtMxLfBTDmcZFWC
                        hex!["eec41f1d016d876f654f247b21813f966a72dd2a60011abed5758a6e26ae7d38"]
                            .into(),
                        // Controller account: 5CtZZkfGJq9AzEn6SndeJyDZkWDNHc1yt99rxKpBAyQbqRKu
                        hex!["247fde0495a574246a1f69bc7a49c752c07a3a82fb2054e40f6d3c9d04e00223"]
                            .into(),
                        // Babe key: 5HTmWmJRVCAaC8unVkjDcQMWhACpiA85ymtMxLfBTDmcZFWC
                        hex!["eec41f1d016d876f654f247b21813f966a72dd2a60011abed5758a6e26ae7d38"]
                            .unchecked_into(),
                        // Grandpa key: 5EFtdrjjDyY7M31kWSLtVKG5t3P8t4Y4d6oic3qKfJ56wFGs
                        hex!["610073bfa83e6d7dc7f4ff4fa28c76141a7f8f4da2f7d227edd6432cbe49db62"]
                            .unchecked_into(),
                    ),
                ],
                // Sudo account
                // 5CtLwzLdsTZnyA3TN7FUV58FV4NZ1tUuTDM9yjwRuvt6ac1i
                hex!["2455655ad2a1f9fbe510699026fc810a2b3cb91d432c141db54a9968da944955"].into(),
                // Pre-funded accounts
                vec![
                    // root_key
                    hex!["2455655ad2a1f9fbe510699026fc810a2b3cb91d432c141db54a9968da944955"].into(),
                ],
                true,
            )
        },
        // Bootnodes
        vec![],
        // Telemetry
        // TODO: define telemetry endpoints
        None,
        // Protocol ID
        None,
        // Fork ID
        None,
        // Properties
        None,
        // Extensions
        Default::default(),
    ))
}

/// Configure initial storage state for FRAME modules.
fn testnet_genesis(
    wasm_binary: &[u8],
    initial_authorities: Vec<(AccountId, AccountId, BabeId, GrandpaId)>,
    root_key: AccountId,
    endowed_accounts: Vec<AccountId>,
    _enable_println: bool,
) -> GenesisConfig {
    const ENDOWMENT: u128 = 1_000_000 * TOKEN;
    const STASH: u128 = 100 * TOKEN;

    GenesisConfig {
        system: SystemConfig {
            // Add Wasm runtime to storage.
            code: wasm_binary.to_vec(),
        },
        balances: BalancesConfig {
            balances: endowed_accounts
                .iter()
                .map(|k: &AccountId| (k.clone(), ENDOWMENT))
                .chain(initial_authorities.iter().map(|x| (x.0.clone(), STASH)))
                .collect(),
        },
        babe: BabeConfig {
            authorities: Default::default(),
            epoch_config: Some(vara_runtime::BABE_GENESIS_EPOCH_CONFIG),
        },
        grandpa: GrandpaConfig {
            authorities: Default::default(),
        },
        session: SessionConfig {
            keys: initial_authorities
                .iter()
                .map(|x| {
                    (
                        x.0.clone(),
                        x.0.clone(),
                        SessionKeys {
                            babe: x.2.clone(),
                            grandpa: x.3.clone(),
                        },
                    )
                })
                .collect::<Vec<_>>(),
        },
        staking: StakingConfig {
            validator_count: initial_authorities.len() as u32,
            minimum_validator_count: initial_authorities.len() as u32,
            stakers: initial_authorities
                .iter()
                .map(|x| (x.0.clone(), x.1.clone(), STASH, StakerStatus::Validator))
                .collect(),
            invulnerables: initial_authorities.iter().map(|x| x.0.clone()).collect(),
            force_era: pallet_staking::Forcing::ForceNone,
            slash_reward_fraction: Perbill::from_percent(10),
            ..Default::default()
        },
        sudo: SudoConfig {
            // Assign network admin rights.
            key: Some(root_key),
        },
        transaction_payment: Default::default(),
        gear: GearConfig {
            force_queue: Default::default(),
        },
    }
}
