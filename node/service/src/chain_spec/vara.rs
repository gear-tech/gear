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

use crate::chain_spec::{get_account_id_from_seed, get_from_seed, AccountId};
use hex_literal::hex;
use sc_service::ChainType;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_core::{crypto::UncheckedInto, sr25519};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use vara_runtime::{
    BabeConfig, BalancesConfig, GenesisConfig, GrandpaConfig, SessionConfig, SessionKeys,
    SudoConfig, SystemConfig, WASM_BINARY,
};

// The URL for the telemetry server.
// const STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<GenesisConfig>;

/// Generate authority keys.
pub fn authority_keys_from_seed(s: &str) -> (AccountId, BabeId, GrandpaId) {
    (
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
                    get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
                    get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
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
        None,
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
                    get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
                    get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
                    get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
                    get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
                    get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
                    get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
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
        None,
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
                        hex!["6efec345ff71786529e5e21ff50fb669a46cb052daa87fd2ce86d9ba4835a533"]
                            .into(),
                        // 5EaEodiLKeJzuW4HACVdFLY5oZnVy9ateRqoM5CM32tfMZhW
                        hex!["6efec345ff71786529e5e21ff50fb669a46cb052daa87fd2ce86d9ba4835a533"]
                            .unchecked_into(),
                        // 5EeZhC24oePqAaYuC9KooYeG2gvWAYnH5s7tXQc9YVVGAGd3
                        hex!["724b54851966f862226e1892975858b59db7dc49f899adbeba305e5275b6c9e3"]
                            .unchecked_into(),
                    ),
                    (
                        hex!["f66b57dee7e59d9288ae6ad9d70d06b7475d01d999a29c35676d7cca3b5fbd6b"]
                            .into(),
                        // 5HdoXNLoGRKGm2Cxpxh7A1vBtbehk8RrKToaQBgK4UhBdVod
                        hex!["f66b57dee7e59d9288ae6ad9d70d06b7475d01d999a29c35676d7cca3b5fbd6b"]
                            .unchecked_into(),
                        // 5FuUkGjuKVWX4RdaQ7PFBZmqVjXMJh7WbV86EKC7iWpBm7ZE
                        hex!["a9e7978e751ad81eda71e6216682674a3f6dbe0c0d0f8f12b83ebec4b7d963c5"]
                            .unchecked_into(),
                    ),
                    (
                        hex!["8ae47a881c08af1eef02292feb9cbdb9cda0e3ee127a07e1bd10f8794a45884c"]
                            .into(),
                        // 5FCpLXt4MgbSm1hbkFpWPVwZxXN7Bz7fYexjUGPBP9XojvJo
                        hex!["8ae47a881c08af1eef02292feb9cbdb9cda0e3ee127a07e1bd10f8794a45884c"]
                            .unchecked_into(),
                        // 5EcaVDoPeacpPAFKjyiYPxt1Et7VPfjjTFBq7wu1ginzacMi
                        hex!["70c782cde31d731ebf9417c80abab1c3945e12eecfdc71adc03e2686fb3a6c1b"]
                            .unchecked_into(),
                    ),
                    (
                        hex!["96edf0641f4f4f387b15870d9610cdfc8db38c701e63b8e863e43e7ff366262b"]
                            .into(),
                        // 5FUbirpEwZtLVbz863h53XbknL7mC1kbZapGmp9PfLws7DL8
                        hex!["96edf0641f4f4f387b15870d9610cdfc8db38c701e63b8e863e43e7ff366262b"]
                            .unchecked_into(),
                        // 5Cocyk8htPZqfzcwmgybXL2wb4Z9uVpNktSXCT2nm5rAewR5
                        hex!["20bb21adf10a8909725498d447f4150a2aec5eca4adfda3321c4b9598298d8a0"]
                            .unchecked_into(),
                    ),
                    (
                        hex!["ee5941d0f4a1f50d70f27a90a655ede3f1dad5ba33a2f8fe9ea5bfe9f0d7c91e"]
                            .into(),
                        // 5HTDmXsmytGu5itJN8WvSXqGpMeD3STFTw7A2G9tbK5foEeM
                        hex!["ee5941d0f4a1f50d70f27a90a655ede3f1dad5ba33a2f8fe9ea5bfe9f0d7c91e"]
                            .unchecked_into(),
                        // 5DyCHLjKgyrr6WXEb16773Vutxu8XYxV81nAaY1aMZHUYFdb
                        hex!["5444aecf3e12dadd4e6f93ca04a7071cda2e7f90e8da7c98f55c27ab291a15f4"]
                            .unchecked_into(),
                    ),
                    (
                        hex!["32b89c4a881f873f33bd18bbcc5b9e571c43e8caa9bd6169ded16e688f0c9d65"]
                            .into(),
                        // 5DDD6JKfrns5WmSSsGZkSmDuc7MePzNHYVuthi8hUnfcCPjv
                        hex!["32b89c4a881f873f33bd18bbcc5b9e571c43e8caa9bd6169ded16e688f0c9d65"]
                            .unchecked_into(),
                        // 5FtffWXRfSJtfuTZqohVdQP44ajjV8ehmAArjRhKJ1mURXfC
                        hex!["a94919797c3cd522ab4de174b9bbd830020372f4c6445ba7d90b491c3547eabf"]
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
        None,
    ))
}

/// Configure initial storage state for FRAME modules.
fn testnet_genesis(
    wasm_binary: &[u8],
    initial_authorities: Vec<(AccountId, BabeId, GrandpaId)>,
    root_key: AccountId,
    endowed_accounts: Vec<AccountId>,
    _enable_println: bool,
) -> GenesisConfig {
    GenesisConfig {
        system: SystemConfig {
            // Add Wasm runtime to storage.
            code: wasm_binary.to_vec(),
        },
        balances: BalancesConfig {
            // Configure endowed accounts with initial balance of 1 << 60.
            balances: endowed_accounts
                .iter()
                .cloned()
                .map(|k| (k, 1 << 60))
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
                            babe: x.1.clone(),
                            grandpa: x.2.clone(),
                        },
                    )
                })
                .collect::<Vec<_>>(),
        },
        sudo: SudoConfig {
            // Assign network admin rights.
            key: Some(root_key),
        },
        transaction_payment: Default::default(),
    }
}
