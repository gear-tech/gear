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
    network::{Ethereum as AlloyEthereum, EthereumWallet, Network, ReceiptResponse},
    primitives::{Address, Bytes, U256, Uint},
    providers::{
        Identity, PendingTransactionBuilder, PendingTransactionError, Provider, ProviderBuilder,
        RootProvider, WalletProvider,
        ext::AnvilApi,
        fillers::{
            BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller,
            SimpleNonceManager, WalletFiller,
        },
    },
    rpc::types::Filter,
    sol_types::{SolEvent, SolValue},
};
use anyhow::Result;
use ethexe_common::ValidatorsVec;
use ethexe_ethereum::{
    Ethereum,
    abi::{
        IWrappedVara,
        middleware_abi::{Gear::SymbioticContracts, IMiddleware},
        symbiotic_abi::{
            DefaultStakerRewardsFactory, DelegatorFactory,
            FullRestakeDelegator::{self, FullRestakeDelegatorInstance},
            OperatorRegistry,
            Slasher::{self, SlasherInstance},
            SlasherFactory,
            Vault::{self as VaultContract, VaultInstance},
            VaultFactory::{self, VaultFactoryInstance},
            VetoSlasher,
            staker_rewards::IDefaultStakerRewards,
        },
    },
    middleware::Middleware,
    wvara,
};

type AlloyRecommendedFillers = JoinFill<
    GasFiller,
    JoinFill<BlobGasFiller, JoinFill<NonceFiller<SimpleNonceManager>, ChainIdFiller>>,
>;
type AlloyProvider = FillProvider<ExeFiller, RootProvider, AlloyEthereum>;

pub(crate) type ExeFiller =
    JoinFill<JoinFill<Identity, AlloyRecommendedFillers>, WalletFiller<EthereumWallet>>;

// VaultInitParams do not contains in [`ethexe_ethereum::abi::Vault`], so we need to define it here.

// abi.encode(
//     address(vault),
//     abi.encode(
//         IFullRestakeDelegator.InitParams({
//             baseParams: IBaseDelegator.BaseParams({
//                 defaultAdminRoleHolder: address(0),
//                 hook: address(0),
//                 hookSetRoleHolder: address(1)
//             }),
//             networkLimitSetRoleHolders: networkLimitSetRoleHolders,
//             operatorNetworkLimitSetRoleHolders: operatorNetworkLimitSetRoleHolders
//         })
//     )
// )
alloy::sol!(
    struct VaultInitParams {
        address collateral;
        address burner;
        uint48 epochDuration;
        bool depositWhitelist;
        bool isDepositLimit;
        uint256 depositLimit;
        address defaultAdminRoleHolder;
        address depositWhitelistSetRoleHolder;
        address depositorWhitelistRoleHolder;
        address isDepositLimitSetRoleHolder;
        address depositLimitSetRoleHolder;
    }

    struct FullRestakeDelegatorBaseParams {
        address defaultAdminRoleHolder;
        address hook;
        address hookSetRoleHolder;
    }

    struct FullRestakeDelegatorInitParams {
        FullRestakeDelegatorBaseParams baseParams;
        address[] networkLimitSetRoleHolders;
        address[] operatorNetworkLimitSetRoleHolders;
    }

    struct SlasherBaseParams {
        bool isBurnerHook;
    }

    struct VetoSlasherInitParams {
        SlasherBaseParams baseParams;
        uint48 vetoDuration;
        uint256 resolverSetEpochsDelay;
    }

    struct DefaultStakerRewardsInitParams {
        address vault;
        uint256 adminFee;
        address defaultAdminRoleHolder;
        address adminFeeClaimRoleHolder;
        address adminFeeSetRoleHolder;
    }
);

// Helper macro rules for sending transactions and calling view functions.
macro_rules! send {
    ($builder:expr) => {
        $builder.send().await.unwrap().get_receipt().await.unwrap()
    };
}

macro_rules! call {
    ($builder:expr) => {
        $builder.call().await.unwrap()
    };
}

#[derive(Copy, Clone)]
pub struct OperatorWithStake(pub Address, pub U256);

/// Represents the vault with the total stake and the list of operators with their stakes.  #[derive(Clone)]
pub struct VaultConfig {
    pub total_vault_stake: U256,
    pub operators: Vec<OperatorWithStake>,
}

pub struct Vault<P: Provider> {
    pub contract: VaultInstance<P>,
}

pub struct SymbioticEnvConfig {
    pub vaults: Vec<VaultConfig>,
}

pub struct SymbioticEnv {
    vaults: Vec<VaultInstance<AlloyProvider>>,
    provider: AlloyProvider,
}

impl SymbioticEnv {
    pub async fn new(config: SymbioticEnvConfig, ethereum: &Ethereum) -> Self {
        let provider: AlloyProvider = ethereum.provider();
        let owner = provider.default_signer_address();
        let middleware_query = ethereum.middleware().query();
        let symbiotic_contracts = middleware_query.symbiotic_contracts().await.unwrap();

        let mut operators = vec![];
        config.vaults.iter().for_each(|vault_cfg| {
            operators.extend(
                vault_cfg
                    .operators
                    .iter()
                    .map(|op| op.0)
                    .collect::<Vec<Address>>(),
            )
        });

        let deployed_vaults = deploy_vaults(
            provider.clone(),
            owner,
            &ethereum,
            config.vaults,
            &symbiotic_contracts,
        )
        .await;

        register_in_middleware(
            provider.clone(),
            ethereum.middleware().address().into(),
            deployed_vaults,
            operators,
        )
        .await;

        Self {
            vaults: vec![],
            provider,
        }
    }
}

/// Deploys the delegator contract for the given vault.
/// Returns the address of the deployed delegator.
async fn deploy_delegator<P>(
    provider: P,
    owner: Address,
    vault_address: Address,
    symbiotic_contracts: &SymbioticContracts,
) -> Address
where
    P: Provider + Clone,
{
    let vault = VaultContract::new(vault_address, provider.clone());
    let delegator_factory_address = call!(vault.DELEGATOR_FACTORY());

    let delegator_factory = DelegatorFactory::new(delegator_factory_address, provider.clone());

    let delegator_type = call!(delegator_factory.totalTypes());
    let delegator_impl = FullRestakeDelegator::deploy(
        provider.clone(),
        symbiotic_contracts.networkRegistry,
        symbiotic_contracts.vaultRegistry,
        Address::ZERO,
        symbiotic_contracts.networkOptIn,
        *delegator_factory.address(),
        delegator_type,
    )
    .await
    .unwrap();

    let _receipt = send!(delegator_factory.whitelist(*delegator_impl.address()));

    let init_params = FullRestakeDelegatorInitParams {
        baseParams: FullRestakeDelegatorBaseParams {
            defaultAdminRoleHolder: owner,
            hook: Address::ZERO,
            hookSetRoleHolder: owner,
        },
        networkLimitSetRoleHolders: vec![owner],
        operatorNetworkLimitSetRoleHolders: vec![owner],
    };

    let data: (Address, Bytes) = (vault_address, init_params.abi_encode().into());
    let encoded_data = data.abi_encode_params().into();

    let receipt = send!(delegator_factory.create(delegator_type, encoded_data));
    let log = receipt.logs()[0]
        .log_decode::<DelegatorFactory::AddEntity>()
        .unwrap();
    log.inner.entity
}

/// Deploys the slasher contract for the given vault.
/// Returns the address of the deployed slasher.
async fn deploy_slasher<P>(
    provider: P,
    vault_address: Address,
    symbiotic_contracts: &SymbioticContracts,
) -> Address
where
    P: Provider + Clone,
{
    let vault = VaultContract::new(vault_address, provider.clone());
    let slasher_factory = SlasherFactory::new(call!(vault.SLASHER_FACTORY()), provider.clone());

    let slasher_type = call!(slasher_factory.totalTypes());
    tracing::info!("Slasher type: {}", slasher_type);

    let slasher_impl = VetoSlasher::deploy(
        provider.clone(),
        symbiotic_contracts.vaultRegistry,
        symbiotic_contracts.middlewareService,
        symbiotic_contracts.networkRegistry,
        *slasher_factory.address(),
        slasher_type,
    )
    .await
    .unwrap();

    let receipt = send!(slasher_factory.whitelist(*slasher_impl.address()));

    let init_params = VetoSlasherInitParams {
        baseParams: SlasherBaseParams {
            isBurnerHook: false,
        },
        vetoDuration: Uint::from(2 * 60 * 60), // 2h (take from `deploy.rs`)
        resolverSetEpochsDelay: Uint::from(5 * 60), // 5 min (take from `deploy.rs`)
    };
    let data: (Address, Bytes) = (vault_address, init_params.abi_encode().into());

    let receipt = send!(slasher_factory.create(slasher_type, data.abi_encode_params().into()));
    let log = receipt.logs()[0]
        .log_decode::<SlasherFactory::AddEntity>()
        .unwrap();
    log.inner.entity
}

async fn deploy_staker_rewards<P>(
    provider: P,
    owner: Address,
    vault_address: Address,
    symbiotic_contracts: &SymbioticContracts,
) -> Address
where
    P: Provider + Clone,
{
    let staker_rewards_factory = DefaultStakerRewardsFactory::new(
        symbiotic_contracts.stakerRewardsFactory,
        provider.clone(),
    );

    let init_params = IDefaultStakerRewards::InitParams {
        vault: vault_address,
        adminFee: Uint::from(1), // 1%
        defaultAdminRoleHolder: owner,
        adminFeeClaimRoleHolder: owner,
        adminFeeSetRoleHolder: owner,
    };

    let receipt = send!(staker_rewards_factory.create(init_params.into()));
    let log = receipt.logs()[0]
        .log_decode::<DefaultStakerRewardsFactory::AddEntity>()
        .unwrap();
    log.inner.entity
}

/// Deploys the vaults contracts for the symbiotic environment.
async fn deploy_vaults<P>(
    provider: P,
    owner: Address,
    ethereum: &Ethereum,
    vaults: Vec<VaultConfig>,
    symbiotic_contracts: &SymbioticContracts,
) -> Vec<Address>
where
    P: WalletProvider + Provider + Clone,
{
    let wvara_addr = ethereum.wrapped_vara().address().into();
    let wvara_instance = IWrappedVara::new(wvara_addr, provider.clone());

    // Up owner's WVARA balance to U256::MAX
    let balance_before = call!(wvara_instance.balanceOf(owner));
    tracing::info!("Owner WVARA balance before mint: {}", balance_before);
    let _receipt = send!(wvara_instance.mint(owner, U256::MAX.saturating_sub(balance_before)));

    let vault_factory = VaultFactory::new(symbiotic_contracts.vaultRegistry, provider.clone());
    let vault_version = call!(vault_factory.lastVersion());

    let vault_init_params = VaultInitParams {
        collateral: wvara_addr,
        burner: Address::repeat_byte(0xab),
        epochDuration: Uint::from(2 * 24 * 60 * 60), // 2 eras
        depositWhitelist: false,
        isDepositLimit: false,
        depositLimit: U256::ZERO,
        defaultAdminRoleHolder: Address::ZERO,
        depositWhitelistSetRoleHolder: Address::ZERO,
        depositorWhitelistRoleHolder: Address::ZERO,
        isDepositLimitSetRoleHolder: owner,
        depositLimitSetRoleHolder: Address::ZERO,
    };

    let mut deployed_vaults = Vec::with_capacity(vaults.len());

    for vault_cfg in vaults {
        let data = vault_init_params.abi_encode().into();
        let receipt = send!(vault_factory.create(vault_version, owner, data));

        tracing::info!("Vault creation logs: {:?}", receipt.logs().len());
        let log = receipt
            .logs()
            .iter()
            .find_map(|log| log.log_decode::<VaultFactory::AddEntity>().ok())
            .unwrap();

        let vault = VaultContract::new(log.inner.entity, provider.clone());

        let _receipt = send!(wvara_instance.approve(*vault.address(), vault_cfg.total_vault_stake));
        let _receipt = send!(vault.deposit(owner, vault_cfg.total_vault_stake));

        let delegator = deploy_delegator(
            provider.clone(),
            owner,
            *vault.address(),
            &symbiotic_contracts,
        )
        .await;

        let _receipt = send!(vault.setDelegator(delegator));

        let slasher =
            deploy_slasher(provider.clone(), *vault.address(), &symbiotic_contracts).await;

        let _receipt = send!(vault.setSlasher(slasher));

        deployed_vaults.push(*vault.address())
    }

    deployed_vaults
}

/// Registers vaults in the middleware contract.
///  Because of `registerVault` requires only in this place so this method not implemented in [`ethexe_ethereum::middleware::Middleware``]
async fn register_in_middleware<P>(
    mut provider: P,
    middleware_address: Address,
    vaults: Vec<Address>,
    operators: Vec<Address>,
) where
    P: Provider + WalletProvider<Wallet = EthereumWallet> + Clone,
{
    let middleware = IMiddleware::new(middleware_address, provider.clone());

    let symbiotic_contracts = call!(middleware.symbioticContracts());

    for vault in vaults {
        let vault_owner = VaultContract::new(vault, provider.clone())
            .owner()
            .call()
            .await
            .unwrap();

        let staker_rewards =
            deploy_staker_rewards(provider.clone(), vault_owner, vault, &symbiotic_contracts).await;

        let signer = provider.default_signer_address();
        tracing::error!("Vault owner: {vault_owner:?}, signer: {signer:?}");
        let _receipt = send!(middleware.registerVault(vault, staker_rewards));
    }

    let default_signer = provider.default_signer_address();

    for operator in operators {
        // Only operator can register itself, so we need to change the signer to the operator address.
        provider.wallet_mut().set_default_signer(operator).unwrap();
        let _receipt = send!(middleware.registerOperator());
    }

    // Restore the default signer.
    provider
        .wallet_mut()
        .set_default_signer(default_signer)
        .unwrap();
}
