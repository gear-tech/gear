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
use gear_runtime_common::{self, constants::BANK_ADDRESS};
use hex_literal::hex;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use pallet_staking::Forcing;
use sc_chain_spec::Properties;
use sc_service::ChainType;
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{crypto::UncheckedInto, sr25519};
use sp_runtime::{Perbill, Perquintill};
use vara_runtime::{
    constants::currency::{ECONOMIC_UNITS, EXISTENTIAL_DEPOSIT, UNITS as TOKEN},
    SessionKeys, StakerStatus, WASM_BINARY,
};

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<(), Extensions>;

/// Returns the [`Properties`] for the Vara network.
pub fn vara_properties() -> Properties {
    let mut p = Properties::new();
    p.insert(
        "ss58format".into(),
        gear_runtime_common::constants::VARA_SS58PREFIX.into(),
    );
    p.insert(
        "tokenDecimals".into(),
        gear_runtime_common::constants::VARA_DECIMAL.into(),
    );
    p.insert(
        "tokenSymbol".into(),
        gear_runtime_common::constants::VARA_TOKEN_SYMBOL.into(),
    );
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
        .with_properties(vara_properties())
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
        .with_properties(vara_properties())
        .build())
}

/// Vara testnet config.
pub fn testnet() -> Result<ChainSpec, String> {
    let wasm_binary =
        WASM_BINARY.ok_or_else(|| "Staging testnet wasm not available".to_string())?;

    Ok(ChainSpec::builder(wasm_binary, Default::default())
        .with_name("Vara Network Testnet")
        .with_id("vara_network_testnet")
        .with_chain_type(ChainType::Live)
        .with_genesis_config_patch(testnet_genesis(
            // Initial PoA authorities
            vec![
                (
                    // Stash account
                    // kGk5PxzdHH3dqmr4sAhrj38WtZe7C6TLcWDK6j1VhMgxyE6Rv
                    hex!["c8e4df7eac6b52dc5281659f1f393903932ee4b1f69f311c3cb123bc40f9267a"].into(),
                    // Controller account
                    // kGk5PxzdHH3dqmr4sAhrj38WtZe7C6TLcWDK6j1VhMgxyE6Rv
                    hex!["c8e4df7eac6b52dc5281659f1f393903932ee4b1f69f311c3cb123bc40f9267a"].into(),
                    // BabeId: kGk5PxzdHH3dqmr4sAhrj38WtZe7C6TLcWDK6j1VhMgxyE6Rv
                    hex!["c8e4df7eac6b52dc5281659f1f393903932ee4b1f69f311c3cb123bc40f9267a"]
                        .unchecked_into(),
                    // GrandpaId: kGkmyeZMTU5LxL52va9WuEvQGzgGr25nAJWhnaZa3uTNogMdy
                    hex!["e7d812ca5322f9b735e6cef4953cb706ce349752d7c737ef7aac817ebb840de1"]
                        .unchecked_into(),
                    // ImOnlineId: kGk5PxzdHH3dqmr4sAhrj38WtZe7C6TLcWDK6j1VhMgxyE6Rv
                    hex!["c8e4df7eac6b52dc5281659f1f393903932ee4b1f69f311c3cb123bc40f9267a"]
                        .unchecked_into(),
                    // AuthorityDiscoveryId: kGk5PxzdHH3dqmr4sAhrj38WtZe7C6TLcWDK6j1VhMgxyE6Rv
                    hex!["c8e4df7eac6b52dc5281659f1f393903932ee4b1f69f311c3cb123bc40f9267a"]
                        .unchecked_into(),
                ),
                (
                    // Stash account
                    // kGgu3wishk7ZwhQLdG6StPR8zNcE1roeeLbTLGxQXiUdrQ6vK
                    hex!["3c4c519e3d7149c93181e8e3762562db6f580c27502e9a6ab2f7464d6185241b"].into(),
                    // Controller account
                    // kGgu3wishk7ZwhQLdG6StPR8zNcE1roeeLbTLGxQXiUdrQ6vK
                    hex!["3c4c519e3d7149c93181e8e3762562db6f580c27502e9a6ab2f7464d6185241b"].into(),
                    // BabeId: kGgu3wishk7ZwhQLdG6StPR8zNcE1roeeLbTLGxQXiUdrQ6vK
                    hex!["3c4c519e3d7149c93181e8e3762562db6f580c27502e9a6ab2f7464d6185241b"]
                        .unchecked_into(),
                    // GrandpaId: kGhkmtR8b94qQhQLVukyTGyexoQyajydms3HFZeYLvxTwaEyt
                    hex!["6238894f19edef4a4a638b3fab9b42909336912bd6ccdf835e9ecc24a64a8713"]
                        .unchecked_into(),
                    // ImOnlineId: kGgu3wishk7ZwhQLdG6StPR8zNcE1roeeLbTLGxQXiUdrQ6vK
                    hex!["3c4c519e3d7149c93181e8e3762562db6f580c27502e9a6ab2f7464d6185241b"]
                        .unchecked_into(),
                    // AuthorityDiscoveryId: kGgu3wishk7ZwhQLdG6StPR8zNcE1roeeLbTLGxQXiUdrQ6vK
                    hex!["3c4c519e3d7149c93181e8e3762562db6f580c27502e9a6ab2f7464d6185241b"]
                        .unchecked_into(),
                ),
                (
                    // Stash account
                    // kGhY2DGv6j9xzy3mJ84R19V9FVDRcxtbZ7QaBQw4tYiFBKzB7
                    hex!["587e919f8149e31f7d4e99e8fbdf30ff119593376f066e20dacda9054892b478"].into(),
                    // Controller account
                    // kGhY2DGv6j9xzy3mJ84R19V9FVDRcxtbZ7QaBQw4tYiFBKzB7
                    hex!["587e919f8149e31f7d4e99e8fbdf30ff119593376f066e20dacda9054892b478"].into(),
                    // BabeId: kGhY2DGv6j9xzy3mJ84R19V9FVDRcxtbZ7QaBQw4tYiFBKzB7
                    hex!["587e919f8149e31f7d4e99e8fbdf30ff119593376f066e20dacda9054892b478"]
                        .unchecked_into(),
                    // GrandpaId: kGjxXy9tDgdthvcxjMacodRwDcF3EkLHw8YBnpKkcGdY8dm8c
                    hex!["c3a91848c88b9481405fb29d07cc221c400763ce3ed3c8735c64a86c026bb5ee"]
                        .unchecked_into(),
                    // ImOnlineId: kGhY2DGv6j9xzy3mJ84R19V9FVDRcxtbZ7QaBQw4tYiFBKzB7
                    hex!["587e919f8149e31f7d4e99e8fbdf30ff119593376f066e20dacda9054892b478"]
                        .unchecked_into(),
                    // AuthorityDiscoveryId: kGhY2DGv6j9xzy3mJ84R19V9FVDRcxtbZ7QaBQw4tYiFBKzB7
                    hex!["587e919f8149e31f7d4e99e8fbdf30ff119593376f066e20dacda9054892b478"]
                        .unchecked_into(),
                ),
                (
                    // Stash account
                    // kGm2bGRMnUR4XBJBTZ46chAypEVSA8NGXwCMzjX5UHnbMwPui
                    hex!["f2fd6936b8ddad025d329ff2d6b5577e6381cb25333f6f17f592494b0b61ef55"].into(),
                    // Controller account
                    // kGm2bGRMnUR4XBJBTZ46chAypEVSA8NGXwCMzjX5UHnbMwPui
                    hex!["f2fd6936b8ddad025d329ff2d6b5577e6381cb25333f6f17f592494b0b61ef55"].into(),
                    // BabeId: kGm2bGRMnUR4XBJBTZ46chAypEVSA8NGXwCMzjX5UHnbMwPui
                    hex!["f2fd6936b8ddad025d329ff2d6b5577e6381cb25333f6f17f592494b0b61ef55"]
                        .unchecked_into(),
                    // GrandpaId: kGhUFt5bRR4D63XbEvGTN9nv8E6X5priTLr2BaxrPdHqN9BMD
                    hex!["559f99f172dcfef6c6894cfe53312b3f11d67c3ac0c29ead872d3ec37f7fcffa"]
                        .unchecked_into(),
                    // ImOnlineId: kGm2bGRMnUR4XBJBTZ46chAypEVSA8NGXwCMzjX5UHnbMwPui
                    hex!["f2fd6936b8ddad025d329ff2d6b5577e6381cb25333f6f17f592494b0b61ef55"]
                        .unchecked_into(),
                    // AuthorityDiscoveryId: kGm2bGRMnUR4XBJBTZ46chAypEVSA8NGXwCMzjX5UHnbMwPui
                    hex!["f2fd6936b8ddad025d329ff2d6b5577e6381cb25333f6f17f592494b0b61ef55"]
                        .unchecked_into(),
                ),
            ],
            // Sudo account
            // kGiV2zSRBY95vX3vsEa4YtvN2jo2pNsJKfUwye9t9w3fbDUnz
            hex!["8273aff7a45330b7eff0807f3a888f442df36d8113e3a62fa99f9737520f1b04"].into(),
            // Pre-funded accounts
            vec![
                // root_key
                hex!["8273aff7a45330b7eff0807f3a888f442df36d8113e3a62fa99f9737520f1b04"].into(),
            ],
            BANK_ADDRESS.into(),
            true,
        ))
        .with_properties(vara_properties())
        .build())
}

/// Vara Network config.
pub fn main() -> Result<ChainSpec, String> {
    let wasm_binary =
        WASM_BINARY.ok_or_else(|| "Staging testnet wasm not available".to_string())?;

    Ok(ChainSpec::builder(wasm_binary, Default::default())
        .with_name("Vara Network")
        .with_id("vara_network")
        .with_chain_type(ChainType::Live)
        .with_genesis_config_patch(testnet_genesis(
            // Initial PoA authorities
            vec![
                (
                    // Stash account
                    // kGi3XM788mBXihSJc4zG2vtUEKYY674SVAYUfqozdS5ea8W6r
                    hex!["6efec345ff71786529e5e21ff50fb669a46cb052daa87fd2ce86d9ba4835a533"].into(),
                    // Controller account
                    // kGkhLheSUxk72NZdk4C9gTiSAvZQjtczMQiJVfiLe7yotnrUy
                    hex!["e44eb7c78c1a46e6d7a92fcc964f5362f0fe9514b58460513f8d051ff79fa95f"].into(),
                    // BabeId: kGi3XM788mBXihSJc4zG2vtUEKYY674SVAYUfqozdS5ea8W6r
                    hex!["6efec345ff71786529e5e21ff50fb669a46cb052daa87fd2ce86d9ba4835a533"]
                        .unchecked_into(),
                    // GrandpaId: kGi7rEfRsFBcYxWoE6w6DV6aQYfg6JTdsbykm29QRwYFAwEdW
                    hex!["724b54851966f862226e1892975858b59db7dc49f899adbeba305e5275b6c9e3"]
                        .unchecked_into(),
                    // ImOnlineId: kGi3XM788mBXihSJc4zG2vtUEKYY674SVAYUfqozdS5ea8W6r
                    hex!["6efec345ff71786529e5e21ff50fb669a46cb052daa87fd2ce86d9ba4835a533"]
                        .unchecked_into(),
                    // AuthorityDiscoveryId: kGi3XM788mBXihSJc4zG2vtUEKYY674SVAYUfqozdS5ea8W6r
                    hex!["6efec345ff71786529e5e21ff50fb669a46cb052daa87fd2ce86d9ba4835a533"]
                        .unchecked_into(),
                ),
                (
                    // Stash account
                    // kGm764qkbhxXzYxTHjkTWqZrLQaQHt3HSqaSStvUbTXT6QSh7
                    hex!["f66b57dee7e59d9288ae6ad9d70d06b7475d01d999a29c35676d7cca3b5fbd6b"].into(),
                    // Controller account
                    // kGgdLoe3ewEo5WK7RyASSzSBGAgcmJaAbhrvcjCsrdx6Grbur
                    hex!["3051267a473a914daab6519d363978f9102e56c0c3ef1be9bc3ae2ce37573630"].into(),
                    // BabeId: kGm764qkbhxXzYxTHjkTWqZrLQaQHt3HSqaSStvUbTXT6QSh7
                    hex!["f66b57dee7e59d9288ae6ad9d70d06b7475d01d999a29c35676d7cca3b5fbd6b"]
                        .unchecked_into(),
                    // GrandpaId: kGjNmHk9hm2jErMsuJu9es7hz1iGwSby77bkxj3zQ7Za6Y4Zh
                    hex!["a9e7978e751ad81eda71e6216682674a3f6dbe0c0d0f8f12b83ebec4b7d963c5"]
                        .unchecked_into(),
                    // ImOnlineId: kGm764qkbhxXzYxTHjkTWqZrLQaQHt3HSqaSStvUbTXT6QSh7
                    hex!["f66b57dee7e59d9288ae6ad9d70d06b7475d01d999a29c35676d7cca3b5fbd6b"]
                        .unchecked_into(),
                    // AuthorityDiscoveryId: kGm764qkbhxXzYxTHjkTWqZrLQaQHt3HSqaSStvUbTXT6QSh7
                    hex!["f66b57dee7e59d9288ae6ad9d70d06b7475d01d999a29c35676d7cca3b5fbd6b"]
                        .unchecked_into(),
                ),
                (
                    // Stash account
                    // kGig6t1HroDpAYwwvf3av53siUW7hKtyG4mbby1BTnCHiWwuZ
                    hex!["8ae47a881c08af1eef02292feb9cbdb9cda0e3ee127a07e1bd10f8794a45884c"].into(),
                    // Controller account
                    // kGggrp4DcKAhC9gzJMsXg4M1rVq7MoEfHsv9xSNym584wkHQ7
                    hex!["32ffe6532fa969364f5b900ddbd5152869a512e1616b7dab8dbfb190e4a06140"].into(),
                    // BabeId: kGig6t1HroDpAYwwvf3av53siUW7hKtyG4mbby1BTnCHiWwuZ
                    hex!["8ae47a881c08af1eef02292feb9cbdb9cda0e3ee127a07e1bd10f8794a45884c"]
                        .unchecked_into(),
                    // GrandpaId: kGi5s2hDC67qYB6VeemUx5Wp9krs5XabKyMphcghJ5mYuMjC8
                    hex!["70c782cde31d731ebf9417c80abab1c3945e12eecfdc71adc03e2686fb3a6c1b"]
                        .unchecked_into(),
                    // ImOnlineId: kGig6t1HroDpAYwwvf3av53siUW7hKtyG4mbby1BTnCHiWwuZ
                    hex!["8ae47a881c08af1eef02292feb9cbdb9cda0e3ee127a07e1bd10f8794a45884c"]
                        .unchecked_into(),
                    // AuthorityDiscoveryId: kGig6t1HroDpAYwwvf3av53siUW7hKtyG4mbby1BTnCHiWwuZ
                    hex!["8ae47a881c08af1eef02292feb9cbdb9cda0e3ee127a07e1bd10f8794a45884c"]
                        .unchecked_into(),
                ),
                (
                    // Stash account
                    // kGiwtGLE3P774HYESzqTUj5XuJJsMKvcC5hT9GYwg4Phmt2m3
                    hex!["96edf0641f4f4f387b15870d9610cdfc8db38c701e63b8e863e43e7ff366262b"].into(),
                    // Controller account
                    // kGkCTbKpa639cuW1GeAKERp64ZS86jKU2WA62HkvpYsCYXigT
                    hex!["ce47cc63787a62acdf9e1d22e295fd4fccd828578ca628c9f9a67f089bf0d07e"].into(),
                    // BabeId: kGiwtGLE3P774HYESzqTUj5XuJJsMKvcC5hT9GYwg4Phmt2m3
                    hex!["96edf0641f4f4f387b15870d9610cdfc8db38c701e63b8e863e43e7ff366262b"]
                        .unchecked_into(),
                    // GrandpaId: kGgGuXDYWKvnZTvsGgUk1Csy673Jk3QfyH15PhBq5A8c5Ruoz
                    hex!["20bb21adf10a8909725498d447f4150a2aec5eca4adfda3321c4b9598298d8a0"]
                        .unchecked_into(),
                    // ImOnlineId: kGiwtGLE3P774HYESzqTUj5XuJJsMKvcC5hT9GYwg4Phmt2m3
                    hex!["96edf0641f4f4f387b15870d9610cdfc8db38c701e63b8e863e43e7ff366262b"]
                        .unchecked_into(),
                    // AuthorityDiscoveryId: kGiwtGLE3P774HYESzqTUj5XuJJsMKvcC5hT9GYwg4Phmt2m3
                    hex!["96edf0641f4f4f387b15870d9610cdfc8db38c701e63b8e863e43e7ff366262b"]
                        .unchecked_into(),
                ),
                (
                    // Stash account
                    // kGkvWK1HaRRVcsf8dGvHL85mRLLPoBMJqz3k2WzxAzMqaZu2v
                    hex!["ee5941d0f4a1f50d70f27a90a655ede3f1dad5ba33a2f8fe9ea5bfe9f0d7c91e"].into(),
                    // Controller account
                    // kGiBGZWA1m55KgCFYh38pTkdJvgJ5ipauWzfpU11FVs3Ntqsw
                    hex!["74e6f377a9181e5d458871ef42d9cc70fccf71ae92be4c2773f0e6bfdf57303b"].into(),
                    // BabeId: kGkvWK1HaRRVcsf8dGvHL85mRLLPoBMJqz3k2WzxAzMqaZu2v
                    hex!["ee5941d0f4a1f50d70f27a90a655ede3f1dad5ba33a2f8fe9ea5bfe9f0d7c91e"]
                        .unchecked_into(),
                    // GrandpaId: kGhSUpp988X5ZtSmZVnrWnbS4QweifTp5e8R35Gorkc3PK8Un
                    hex!["5444aecf3e12dadd4e6f93ca04a7071cda2e7f90e8da7c98f55c27ab291a15f4"]
                        .unchecked_into(),
                    // ImOnlineId: kGkvWK1HaRRVcsf8dGvHL85mRLLPoBMJqz3k2WzxAzMqaZu2v
                    hex!["ee5941d0f4a1f50d70f27a90a655ede3f1dad5ba33a2f8fe9ea5bfe9f0d7c91e"]
                        .unchecked_into(),
                    // AuthorityDiscoveryId: kGkvWK1HaRRVcsf8dGvHL85mRLLPoBMJqz3k2WzxAzMqaZu2v
                    hex!["ee5941d0f4a1f50d70f27a90a655ede3f1dad5ba33a2f8fe9ea5bfe9f0d7c91e"]
                        .unchecked_into(),
                ),
                (
                    // Stash account
                    // kGggVdmjUJL5oJhgmn4LA8KA4867EXuDt4cYmCSvysqRWyUwi
                    hex!["32b89c4a881f873f33bd18bbcc5b9e571c43e8caa9bd6169ded16e688f0c9d65"].into(),
                    // Controller account
                    // kGgujsZrbNWWWhe6HfYCpXaVWxZhCUXndNp6fupiGdZr1WBez
                    hex!["3cd2bac9ade1bc68c9e75d67c9aa9d021cb4c46ef16ba7a6ee8c1d351faa750f"].into(),
                    // BabeId: kGggVdmjUJL5oJhgmn4LA8KA4867EXuDt4cYmCSvysqRWyUwi
                    hex!["32b89c4a881f873f33bd18bbcc5b9e571c43e8caa9bd6169ded16e688f0c9d65"]
                        .unchecked_into(),
                    // GrandpaId: kGjMxCywE6yXcTqhtkbTuJxKCaZVKd3WJHGojEAVbh4XPCmGo
                    hex!["a94919797c3cd522ab4de174b9bbd830020372f4c6445ba7d90b491c3547eabf"]
                        .unchecked_into(),
                    // ImOnlineId: kGggVdmjUJL5oJhgmn4LA8KA4867EXuDt4cYmCSvysqRWyUwi
                    hex!["32b89c4a881f873f33bd18bbcc5b9e571c43e8caa9bd6169ded16e688f0c9d65"]
                        .unchecked_into(),
                    // AuthorityDiscoveryId: kGggVdmjUJL5oJhgmn4LA8KA4867EXuDt4cYmCSvysqRWyUwi
                    hex!["32b89c4a881f873f33bd18bbcc5b9e571c43e8caa9bd6169ded16e688f0c9d65"]
                        .unchecked_into(),
                ),
                (
                    // Stash account
                    // kGhxBx6wT8TuTTp7Kq5Vo6YgG2GeWqqGMAvYQ1iezngp542F7
                    hex!["6aed3db006563f67b75bd1c6cc2129eab6cdc0aac34281a50ea78c2b4d38fa5d"].into(),
                    // Controller account
                    // kGgpP8eJTF2pL81ifdRBJJDf32VTDR4drcmTFS34t3PppnRsW
                    hex!["38bcaf73c4c539cb055f81e0965379d189edf7687e5d7d4088b514acc0654a64"].into(),
                    // kGhxBx6wT8TuTTp7Kq5Vo6YgG2GeWqqGMAvYQ1iezngp542F7
                    hex!["6aed3db006563f67b75bd1c6cc2129eab6cdc0aac34281a50ea78c2b4d38fa5d"]
                        .unchecked_into(),
                    // kGj1iR5fMqyBBZR7eKk92wU39upaPFgf8YzoWrrc2x9KpB1Wk
                    hex!["99d9c3f315705920228b49ad2b0d68ef2dc4cc1b6d9e395e93e9b56e224ec549"]
                        .unchecked_into(),
                    // kGhxBx6wT8TuTTp7Kq5Vo6YgG2GeWqqGMAvYQ1iezngp542F7
                    hex!["6aed3db006563f67b75bd1c6cc2129eab6cdc0aac34281a50ea78c2b4d38fa5d"]
                        .unchecked_into(),
                    // kGhxBx6wT8TuTTp7Kq5Vo6YgG2GeWqqGMAvYQ1iezngp542F7
                    hex!["6aed3db006563f67b75bd1c6cc2129eab6cdc0aac34281a50ea78c2b4d38fa5d"]
                        .unchecked_into(),
                ),
                (
                    // Stash account
                    // kGgmgGKJHRMLkYRnjNSmu6ntrgNnhMdizphbpJSXAKv7sm5Yf
                    hex!["36ac9f1de2c59b1d175644c809765abaa9c18aa844d579c7a988a28be1d61336"].into(),
                    // Controller account
                    // kGjSj5gxudJBbwqQx7W8CcQ52QeBdKoTU9E7WyPBrcAUhFDEc
                    hex!["aced2430dcf00a89a4d9339ba01a6a1fad80f549768b05fdcb2b0a33fb6aec5b"].into(),
                    // kGgmgGKJHRMLkYRnjNSmu6ntrgNnhMdizphbpJSXAKv7sm5Yf
                    hex!["36ac9f1de2c59b1d175644c809765abaa9c18aa844d579c7a988a28be1d61336"]
                        .unchecked_into(),
                    // kGmAAhMP1Lyt8jhYugXccrjYMTCHeNGm4Fvx5nLcdE6VjnQTG
                    hex!["f8c4a9ea78f44b4e0ce7dcf37359f0a7e8a0ab5d956d9dbc177c3606bf874412"]
                        .unchecked_into(),
                    // kGgmgGKJHRMLkYRnjNSmu6ntrgNnhMdizphbpJSXAKv7sm5Yf
                    hex!["36ac9f1de2c59b1d175644c809765abaa9c18aa844d579c7a988a28be1d61336"]
                        .unchecked_into(),
                    // kGgmgGKJHRMLkYRnjNSmu6ntrgNnhMdizphbpJSXAKv7sm5Yf
                    hex!["36ac9f1de2c59b1d175644c809765abaa9c18aa844d579c7a988a28be1d61336"]
                        .unchecked_into(),
                ),
                (
                    // Stash account
                    // kGkt6yGT1LmGWYnFoLJdtuuk8LbarJvwXCSGFiu9ivFG13mXx
                    hex!["ec84321d9751c066fb923035073a73d467d44642c457915e7496c52f45db1f65"].into(),
                    // Controller account
                    // kGg65JSrkz9R85RU8pudSRCJsQvrD3H3tJRqP87Dt2DWaRS73
                    hex!["18785a9a9853652d403cfa7e89afb873c22c53e2f153c9fa5af856028de6a75f"].into(),
                    // BabeId: kGkt6yGT1LmGWYnFoLJdtuuk8LbarJvwXCSGFiu9ivFG13mXx
                    hex!["ec84321d9751c066fb923035073a73d467d44642c457915e7496c52f45db1f65"]
                        .unchecked_into(),
                    // GrandpaId: kGgrUdLxC4wsfJ8316uRprin3ouh73oMVRuTtnDemydVmFdwL
                    hex!["3a55ac67c147af497e9dc14debf7d5674969cc7cb2099fdf598ee6a7c36fe3b4"]
                        .unchecked_into(),
                    // ImOnlineId: kGkt6yGT1LmGWYnFoLJdtuuk8LbarJvwXCSGFiu9ivFG13mXx
                    hex!["ec84321d9751c066fb923035073a73d467d44642c457915e7496c52f45db1f65"]
                        .unchecked_into(),
                    // AuthorityDiscoveryId: kGkt6yGT1LmGWYnFoLJdtuuk8LbarJvwXCSGFiu9ivFG13mXx
                    hex!["ec84321d9751c066fb923035073a73d467d44642c457915e7496c52f45db1f65"]
                        .unchecked_into(),
                ),
                (
                    // Stash account
                    // kGm9advPW8FVJQVnvTwB7ijbd37Es1CReCcdJy5EAJFLF5mPu
                    hex!["f85202a9d5727171623a417147625dcd317c7ecb7ce79f8b664dfac093efda19"].into(),
                    // Controller account
                    // kGfgmBYai833QG8pdFeCAho73nut7a4Zrx8YYsm1dL7xQrEhc
                    hex!["06b0b7361b821f19c84c05a558d60a44a52d7ae350c3637b65df40baf66f4a64"].into(),
                    // BabeId: kGm9advPW8FVJQVnvTwB7ijbd37Es1CReCcdJy5EAJFLF5mPu
                    hex!["f85202a9d5727171623a417147625dcd317c7ecb7ce79f8b664dfac093efda19"]
                        .unchecked_into(),
                    // GrandpaId: kGkiiv3nDz2dmZZbkqPZGH73Mb4vWUpx451CoweFq4qpbo9QG
                    hex!["e55cbde1cf31fe6b891ac4cffcce790015e77ddd0f6943653e9b4d722f72baa4"]
                        .unchecked_into(),
                    // ImOnlineId: kGm9advPW8FVJQVnvTwB7ijbd37Es1CReCcdJy5EAJFLF5mPu
                    hex!["f85202a9d5727171623a417147625dcd317c7ecb7ce79f8b664dfac093efda19"]
                        .unchecked_into(),
                    // AuthorityDiscoveryId: kGm9advPW8FVJQVnvTwB7ijbd37Es1CReCcdJy5EAJFLF5mPu
                    hex!["f85202a9d5727171623a417147625dcd317c7ecb7ce79f8b664dfac093efda19"]
                        .unchecked_into(),
                ),
                (
                    // Stash account: kGhxAkgEraid83wXjfRL2gQEahxYY2VcUVP1Us9UzkUEVwuHU
                    hex!["6ae93625c928a59f1bf9f1c01548bbd72d9bb356c56c2bb070dda79590fd4a7f"].into(),
                    // Controller account: kGjSTB11PhY9TjzrZvsgCmYeKkCEJabmDPXam1xBkctXFUvgX
                    hex!["acb796bd17e05ea7c1764355d3c524d8379dc88b910467379afab52776d8616a"].into(),
                    // Babe key: kGhxAkgEraid83wXjfRL2gQEahxYY2VcUVP1Us9UzkUEVwuHU
                    hex!["6ae93625c928a59f1bf9f1c01548bbd72d9bb356c56c2bb070dda79590fd4a7f"]
                        .unchecked_into(),
                    // Grandpa key: kGgZopsg1gEytoxPRQKkynJxphe9WpjTCRHD9obzPRqKRrDrE
                    hex!["2d9f2166122f449c2dcb92d4de97cca7043158968d82e27bacade4015ec55b00"]
                        .unchecked_into(),
                    // ImOnline key: kGhxAkgEraid83wXjfRL2gQEahxYY2VcUVP1Us9UzkUEVwuHU
                    hex!["6ae93625c928a59f1bf9f1c01548bbd72d9bb356c56c2bb070dda79590fd4a7f"]
                        .unchecked_into(),
                    // AuthorityDiscovery key: kGhxAkgEraid83wXjfRL2gQEahxYY2VcUVP1Us9UzkUEVwuHU
                    hex!["6ae93625c928a59f1bf9f1c01548bbd72d9bb356c56c2bb070dda79590fd4a7f"]
                        .unchecked_into(),
                ),
                (
                    // Stash account: kGkw44EiDvjPHz5A7QYVdHxHfD8xQr4ygVtXET5TTrGXXLRz9
                    hex!["eec41f1d016d876f654f247b21813f966a72dd2a60011abed5758a6e26ae7d38"].into(),
                    // Controller account: kGgMr7E54kNMtnB2RMaQ3zX9iGUxxRWsaQFnjT4cTa2AWcFx6
                    hex!["247fde0495a574246a1f69bc7a49c752c07a3a82fb2054e40f6d3c9d04e00223"].into(),
                    // Babe key: kGkw44EiDvjPHz5A7QYVdHxHfD8xQr4ygVtXET5TTrGXXLRz9
                    hex!["eec41f1d016d876f654f247b21813f966a72dd2a60011abed5758a6e26ae7d38"]
                        .unchecked_into(),
                    // Grandpa key: kGhjBBL9XfWkq8yG5RE7JAsCEQ28j1yPf9DSb6ndc4Lq1iKTU
                    hex!["610073bfa83e6d7dc7f4ff4fa28c76141a7f8f4da2f7d227edd6432cbe49db62"]
                        .unchecked_into(),
                    // ImOnline key: kGkw44EiDvjPHz5A7QYVdHxHfD8xQr4ygVtXET5TTrGXXLRz9
                    hex!["eec41f1d016d876f654f247b21813f966a72dd2a60011abed5758a6e26ae7d38"]
                        .unchecked_into(),
                    // AuthorityDiscovery key: kGkw44EiDvjPHz5A7QYVdHxHfD8xQr4ygVtXET5TTrGXXLRz9
                    hex!["eec41f1d016d876f654f247b21813f966a72dd2a60011abed5758a6e26ae7d38"]
                        .unchecked_into(),
                ),
                (
                    // Stash account: kGkRf6uswrEEnfzAXY8UbiKhqbW6uwG2ZhXRs28hJgzJdh4A9
                    hex!["d858bc34e0aa888b6b5f8ce10b6db1112526049cb4c52ef95dfb1c9b10494818"].into(),
                    // Controller account: kGkyLUqpzswKFAE3JE2FGwuRX9537wi2gbw7MiwoUBnA6y2rx
                    hex!["f081e6b796bdd0b7f6217d67f75cd545d7c6224cde534f1edc442ce596bf6c77"].into(),
                    // Babe key: kGkRf6uswrEEnfzAXY8UbiKhqbW6uwG2ZhXRs28hJgzJdh4A9
                    hex!["d858bc34e0aa888b6b5f8ce10b6db1112526049cb4c52ef95dfb1c9b10494818"]
                        .unchecked_into(),
                    // Grandpa key: kGmJYPMMbn6aAUzf7xg7C2AKWdKxvjwjTEMcHqGumFDA7myvK
                    hex!["ff27a40d9901dfbec094c38c0f884efa96168445b206a8b7a1fb8c80301996a5"]
                        .unchecked_into(),
                    // ImOnline key: kGkRf6uswrEEnfzAXY8UbiKhqbW6uwG2ZhXRs28hJgzJdh4A9
                    hex!["d858bc34e0aa888b6b5f8ce10b6db1112526049cb4c52ef95dfb1c9b10494818"]
                        .unchecked_into(),
                    // AuthorityDiscovery key: kGkRf6uswrEEnfzAXY8UbiKhqbW6uwG2ZhXRs28hJgzJdh4A9
                    hex!["d858bc34e0aa888b6b5f8ce10b6db1112526049cb4c52ef95dfb1c9b10494818"]
                        .unchecked_into(),
                ),
                (
                    // Stash account: kGk2TDx8rZrxcwr2aoDxLkaF5d3vkqMTxGUu4LibPq9VKJUG1
                    hex!["c6a61a93bd2261b7667ec4ab812c71bba4cfae3e1d376b9dd52ade4652dcc151"].into(),
                    // Controller account: kGiBPzdDoqQbC11r24arKiAPCSLfjdGAWTtuy5ACJ1eM9dHkj
                    hex!["74fff92414ef779a9c39a32cc740da6f89e8e0c37ef8935ae96cc90845f1830f"].into(),
                    // Babe key: kGk2TDx8rZrxcwr2aoDxLkaF5d3vkqMTxGUu4LibPq9VKJUG1
                    hex!["c6a61a93bd2261b7667ec4ab812c71bba4cfae3e1d376b9dd52ade4652dcc151"]
                        .unchecked_into(),
                    // Grandpa key: kGhtqJgG4qabpb7V78uirv7W9iPBARy36znYwahbbMsQaNTLh
                    hex!["685e01afd77c4c4c577d2380767ec1549114e86513f0b6ce31be96b5b45ad99c"]
                        .unchecked_into(),
                    // ImOnline key: kGk2TDx8rZrxcwr2aoDxLkaF5d3vkqMTxGUu4LibPq9VKJUG1
                    hex!["c6a61a93bd2261b7667ec4ab812c71bba4cfae3e1d376b9dd52ade4652dcc151"]
                        .unchecked_into(),
                    // AuthorityDiscovery key: kGk2TDx8rZrxcwr2aoDxLkaF5d3vkqMTxGUu4LibPq9VKJUG1
                    hex!["c6a61a93bd2261b7667ec4ab812c71bba4cfae3e1d376b9dd52ade4652dcc151"]
                        .unchecked_into(),
                ),
                (
                    // Stash account: kGkRHtZZWpwXpCHLGMXMKyTe8mL6gseT2eBBX3Da5XXVfX6ky
                    hex!["d81153798064bd066022258057680b0cfe2db6e8b9c96995d6216a39b687881d"].into(),
                    // Controller account: kGjV2YVGzTWQBfbHLsJRZbyXGp6VHP8hK9wNbLwnUAjGmz7aV
                    hex!["aeae6a26d64a51c4afcb2bcd2546f63162f1130d1670a366ef6a643b8443a546"].into(),
                    // Babe key: kGkRHtZZWpwXpCHLGMXMKyTe8mL6gseT2eBBX3Da5XXVfX6ky
                    hex!["d81153798064bd066022258057680b0cfe2db6e8b9c96995d6216a39b687881d"]
                        .unchecked_into(),
                    // Grandpa key: kGj26nHC5VGor2QyX8AGfaXbyKcfETUzQRCgC9ZE7JKA7awb7
                    hex!["9a250ded2628e6ae38ee0ddf9d0b081801f6c50333418214f0671f5cf8b8149e"]
                        .unchecked_into(),
                    // ImOnline account: kGkRHtZZWpwXpCHLGMXMKyTe8mL6gseT2eBBX3Da5XXVfX6ky
                    hex!["d81153798064bd066022258057680b0cfe2db6e8b9c96995d6216a39b687881d"]
                        .unchecked_into(),
                    // AuthorityDiscovery account: kGkRHtZZWpwXpCHLGMXMKyTe8mL6gseT2eBBX3Da5XXVfX6ky
                    hex!["d81153798064bd066022258057680b0cfe2db6e8b9c96995d6216a39b687881d"]
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
            BANK_ADDRESS.into(),
            true,
        ))
        .with_properties(vara_properties())
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
