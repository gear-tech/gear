// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use alloy::{
    network::{EthereumWallet, Network, TxSigner},
    node_bindings::Anvil,
    primitives::{Address as AlloyAddress, Bytes, Uint},
    providers::{
        Provider, ProviderBuilder, WalletProvider,
        fillers::{SimpleNonceManager, WalletFiller},
    },
    sol,
    sol_types::SolCall,
};
use anyhow::Result;
use ethexe_common::Address;
use gprimitives::U256;
use std::collections::BTreeMap;

// # Symbiotic contracts for testing
sol!(
    #[sol(rpc)]
    DefaultOperatorRewards,
    "../contracts/lib/symbiotic-rewards/out/DefaultOperatorRewards.sol/DefaultOperatorRewards.json"
);

sol!(
    #[sol(rpc)]
    NetworkMiddlewareService,
    "../contracts/lib/symbiotic-core/out/NetworkMiddlewareService.sol/NetworkMiddlewareService.json"
);

sol!(
    #[sol(rpc)]
    NetworkRegistry,
    "../contracts/lib/symbiotic-core/out/NetworkRegistry.sol/NetworkRegistry.json"
);

sol!(
    #[sol(rpc)]
    Vault,
    "../contracts/lib/symbiotic-core/out/Vault.sol/Vault.json"
);

sol!(
    #[allow(clippy::too_many_arguments)]
    #[sol(rpc)]
    WVara,
    "../ethereum/WrappedVara.json"
);

sol!(
    #[sol(rpc)]
    TransparentUpgradeableProxy,
    "../ethereum/TransparentUpgradeableProxy.json"
);

// Macro replaces the boilerplate code for sending a transaction call builder and waiting for the receipt.
macro_rules! send {
    ($builder:expr) => {
        $builder.send().await.unwrap().get_receipt().await.unwrap()
    };
}

// This function creates a new instance of the `DefaultOperatorRewards` contract with the given signer and provider.
// It is needed to interact with the contract from the perspective of a specific operator.
fn new_rewards_instance<P, N>(
    signer: AlloyAddress,
    rewards_contract: AlloyAddress,
    mut provider: P,
) -> Result<DefaultOperatorRewards::DefaultOperatorRewardsInstance<P, N>>
where
    P: WalletProvider<N, Wallet = EthereumWallet> + Provider<N> + Clone,
    N: Network,
{
    provider.wallet_mut().set_default_signer(signer)?;
    Ok(DefaultOperatorRewards::new(
        rewards_contract,
        provider.clone(),
    ))
}

#[tokio::test]
async fn test_claim_rewards() -> Result<()> {
    let anvil = Anvil::new().keep_stdout().spawn();
    let provider = ProviderBuilder::new()
        .with_nonce_management(SimpleNonceManager::default())
        .filler(WalletFiller::new(anvil.wallet().unwrap()))
        .connect_anvil();

    let signers = provider.signer_addresses().collect::<Vec<_>>();
    assert!(
        !signers.is_empty(),
        "Expect at least one signer in anvil wallet"
    );

    let deployer_address = provider.wallet().default_signer().address();

    // Deploy contracts
    let wvara_impl = WVara::deploy(provider.clone()).await?;
    let wvara_proxy = TransparentUpgradeableProxy::deploy(
        provider.clone(),
        *wvara_impl.address(),
        deployer_address,
        Bytes::copy_from_slice(
            &WVara::initializeCall {
                initialOwner: deployer_address,
            }
            .abi_encode(),
        ),
    )
    .await?;
    let wvara = WVara::new(*wvara_proxy.address(), provider.clone());

    let network_registry = NetworkRegistry::deploy(provider.clone()).await.unwrap();
    let network_middleware_service =
        NetworkMiddlewareService::deploy(provider.clone(), *network_registry.address()).await?;
    let default_operator_rewards =
        DefaultOperatorRewards::deploy(provider.clone(), *network_middleware_service.address())
            .await?;

    // Setup registries
    // Here we register the network with address `deployer_address` (msg.sender)
    // Network middleware also will be set to the `deployer_address`
    send!(network_registry.registerNetwork());
    send!(network_middleware_service.setMiddleware(deployer_address));
    send!(wvara.approve(*default_operator_rewards.address(), Uint::<256, 4>::MAX));

    // Create merkle rewards merkle tree
    let rewards = signers
        .iter()
        .enumerate()
        .map(|(i, key)| (Address(key.0.0), U256::from((i + 1) * 100)))
        .collect::<BTreeMap<_, _>>();
    let tree = crate::rewards::utils::build_merkle_tree(rewards.clone());

    let owner_balance = wvara.balanceOf(deployer_address).call().await?;
    send!(default_operator_rewards.distributeRewards(
        deployer_address,
        *wvara.address(),
        owner_balance,
        tree.get_root().unwrap().0.into(),
    ));

    for (address, to_claim) in rewards.iter() {
        let balance_before = wvara.balanceOf(address.0.into()).call().await?;
        assert!(balance_before.is_zero(), "Operator balance should be 0");

        let new_instance = new_rewards_instance(
            address.0.into(),
            *default_operator_rewards.address(),
            provider.clone(),
        )?;

        let node = oz_merkle_rs::MerkleTree::hash_node((address.0.into(), *to_claim));
        let proof_hashes = tree.get_proof(node).unwrap();
        send!(new_instance.claimRewards(
            address.0.into(),
            deployer_address,
            *wvara.address(),
            Uint::from_limbs(to_claim.0),
            proof_hashes.iter().map(|hash| hash.0.into()).collect(),
        ));

        let balance_after = wvara
            .balanceOf(AlloyAddress(address.0.into()))
            .call()
            .await?;
        assert_eq!(
            balance_after,
            Uint::from_limbs(to_claim.0),
            "Operator balance should match claimed amount"
        );
    }

    Ok(())
}
