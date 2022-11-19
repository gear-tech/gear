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
use gear_runtime::{
    BabeConfig, BalancesConfig, GearConfig, GenesisConfig, GrandpaConfig, SessionConfig,
    SessionKeys, SudoConfig, SystemConfig, WASM_BINARY,
};
use hex_literal::hex;
use sc_service::ChainType;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_core::{crypto::UncheckedInto, sr25519};
use sp_finality_grandpa::AuthorityId as GrandpaId;

// The URL for the telemetry server.
// const STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<GenesisConfig, Extensions>;

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
        "gear_dev",
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
        Default::default(),
    ))
}

pub fn local_testnet_config() -> Result<ChainSpec, String> {
    let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;

    Ok(ChainSpec::from_genesis(
        // Name
        "Gear Local Testnet",
        // ID
        "gear_local_testnet",
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
        Default::default(),
    ))
}

/// Staging testnet config.
pub fn staging_testnet_config() -> Result<ChainSpec, String> {
    let wasm_binary =
        WASM_BINARY.ok_or_else(|| "Staging testnet wasm not available".to_string())?;

    Ok(ChainSpec::from_genesis(
        "Gear Staging Testnet V4",
        "gear_staging_testnet_v4",
        ChainType::Live,
        move || {
            testnet_genesis(
                wasm_binary,
                // Initial PoA authorities
                vec![
                    (
                        // 5Gc7RXDUqWR7yupYFv6KMaak3fMbxYV1z6gEEGhQxeD4TBj9
                        hex!["c8e4df7eac6b52dc5281659f1f393903932ee4b1f69f311c3cb123bc40f9267a"]
                            .into(),
                        // 5Gc7RXDUqWR7yupYFv6KMaak3fMbxYV1z6gEEGhQxeD4TBj9
                        hex!["c8e4df7eac6b52dc5281659f1f393903932ee4b1f69f311c3cb123bc40f9267a"]
                            .unchecked_into(),
                        // 5HJh75wf2Y8EY8nbfMkVZNU8UhXFtAvZnQ4v5pmmWQctuNun
                        hex!["e7d812ca5322f9b735e6cef4953cb706ce349752d7c737ef7aac817ebb840de1"]
                            .unchecked_into(),
                    ),
                    (
                        // 5DRmQFTuJaMDuU6JMJgUhsCqrdURito3pUpTnDcFKRswdGXz
                        hex!["3c4c519e3d7149c93181e8e3762562db6f580c27502e9a6ab2f7464d6185241b"]
                            .into(),
                        // 5DRmQFTuJaMDuU6JMJgUhsCqrdURito3pUpTnDcFKRswdGXz
                        hex!["3c4c519e3d7149c93181e8e3762562db6f580c27502e9a6ab2f7464d6185241b"]
                            .unchecked_into(),
                        // 5EHVLwinhXcguU6AzyD3bRipHSDzc4nBLveP4uk4Xui2oW1b
                        hex!["6238894f19edef4a4a638b3fab9b42909336912bd6ccdf835e9ecc24a64a8713"]
                            .unchecked_into(),
                    ),
                    (
                        // 5E4jfoWJHckHB7WyDGebTwD6yEg2pyjxbHwJvCGc9fVGZ3GN
                        hex!["587e919f8149e31f7d4e99e8fbdf30ff119593376f066e20dacda9054892b478"]
                            .into(),
                        // 5E4jfoWJHckHB7WyDGebTwD6yEg2pyjxbHwJvCGc9fVGZ3GN
                        hex!["587e919f8149e31f7d4e99e8fbdf30ff119593376f066e20dacda9054892b478"]
                            .unchecked_into(),
                        // 5GVFRgURF6fz8giQSnrPwt156GHecRSLcRYvKaxKsanDroFS
                        hex!["c3a91848c88b9481405fb29d07cc221c400763ce3ed3c8735c64a86c026bb5ee"]
                            .unchecked_into(),
                    ),
                    (
                        // 5HZJiwwz2sqoPMw8eGLD1d3fiWgZzTQwR5j8EnHBtjqTAUqq
                        hex!["f2fd6936b8ddad025d329ff2d6b5577e6381cb25333f6f17f592494b0b61ef55"]
                            .into(),
                        // 5HZJiwwz2sqoPMw8eGLD1d3fiWgZzTQwR5j8EnHBtjqTAUqq
                        hex!["f2fd6936b8ddad025d329ff2d6b5577e6381cb25333f6f17f592494b0b61ef55"]
                            .unchecked_into(),
                        // 5DzyLcBcyWzNFbLv1UgxUEyyi7mVgwrrpjPK6E47EF5TNDMJ
                        hex!["559f99f172dcfef6c6894cfe53312b3f11d67c3ac0c29ead872d3ec37f7fcffa"]
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

/// Stable testnet config.
pub fn stable_testnet_config() -> Result<ChainSpec, String> {
    let wasm_binary =
        WASM_BINARY.ok_or_else(|| "Staging testnet wasm not available".to_string())?;

    Ok(ChainSpec::from_genesis(
        "Gear Stable Testnet",
        "gear_stable_testnet",
        ChainType::Live,
        move || {
            testnet_genesis(
                wasm_binary,
                // Initial PoA authorities
                vec![
                    // Validator #1
                    (
                        // 5EUwhQKNFEcctknLMoJrJHggUTsGBYUVdrZg5f8QpAA8syRp
                        hex!["6af4f26ff24a2327847c0863e215b24b896ad6cfa5d3db008a9693c16d82004c"]
                            .into(),
                        hex!["6af4f26ff24a2327847c0863e215b24b896ad6cfa5d3db008a9693c16d82004c"]
                            .unchecked_into(),
                        // 5DMGvYcyRXHRH2eonvBnU3ParGEtpoRSH6XhRp9bYrGw5ryw
                        hex!["38df790d497e0b6e8e9b36cd171d6271dde4af3771dddc6fdd21734b8d8b2288"]
                            .unchecked_into(),
                    ),
                    // Validator #2
                    (
                        // 5HgoExa5XFnJGfZAr8f8KQbeJGti2ygpogMH5YPa9QPEaU1L
                        hex!["f8b4208573d0fee84c13e13f9454ce1ff20d27a2d31a38eed2f706d835f7837d"]
                            .into(),
                        hex!["f8b4208573d0fee84c13e13f9454ce1ff20d27a2d31a38eed2f706d835f7837d"]
                            .unchecked_into(),
                        // 5EUsLw6aJzyoW6ZWKPsakv4cF6Q6WhUrUV2PuaYEHw6i6dx6
                        hex!["6ae64b35b1f99f810d313531d3f1064164202cba925a661cc2da8e2ce166faea"]
                            .unchecked_into(),
                    ),
                    // Validator #3
                    (
                        // 5H3y3g259pzbbK1ki92preL2HLdAhwDM1ZGFPN97DUGbymG8
                        hex!["dc9d0e197b4aed5eb6b0fc0e0748478da177c548e2b017d883bda6a07b880e72"]
                            .into(),
                        hex!["dc9d0e197b4aed5eb6b0fc0e0748478da177c548e2b017d883bda6a07b880e72"]
                            .unchecked_into(),
                        // 5FULaFGRauxgdZ3K61h55dtRiKg8e83yv5Kqr7ENN2PZmoXA
                        hex!["96baf1d3b2983822f1481bebc3679b158eb41d9bd0b9ff102b53129c429a4e85"]
                            .unchecked_into(),
                    ),
                    // Validator #4
                    (
                        // 5D7Gg9MZ6ZvKkR7r1tmck3ayh1HhDqdP4yvN4XsvVZqnhRwG
                        hex!["2e31338c64a9472fc00665ae3bdec5ef353837d566dd5c67f0b8f9f1161f5b28"]
                            .into(),
                        hex!["2e31338c64a9472fc00665ae3bdec5ef353837d566dd5c67f0b8f9f1161f5b28"]
                            .unchecked_into(),
                        // 5EKTZWac3Q5GisMTsvJpR3kj8h1TLHCDMqTM3hWstFj5VE5C
                        hex!["63b9062036f9e39fef48b4c9cb12a5df8e14a8d089ed7d5bd94eef7862626e32"]
                            .unchecked_into(),
                    ),
                ],
                // Sudo account
                // 5HBAzrRWFkhtW8PwcZWJt25N6JG2uYFT25A2LGXAt5CEZy8X
                hex!["e21c013a912424cad0b3bc592d872c3c953a1b6214adbc61cf0a0ef3145ea67c"].into(),
                // Pre-funded accounts
                vec![
                    // root_key
                    hex!["e21c013a912424cad0b3bc592d872c3c953a1b6214adbc61cf0a0ef3145ea67c"].into(),
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
            epoch_config: Some(gear_runtime::BABE_GENESIS_EPOCH_CONFIG),
        },
        grandpa: GrandpaConfig {
            authorities: Default::default(),
        },
        session: SessionConfig {
            keys: initial_authorities
                .into_iter()
                .map(|x| {
                    (
                        x.0.clone(),
                        x.0,
                        SessionKeys {
                            babe: x.1,
                            grandpa: x.2,
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
        gear: GearConfig {
            force_queue: Default::default(),
        },
    }
}
