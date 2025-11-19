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

use crate::{
    AlloyProvider, Ethereum, TryGetReceipt,
    abi::{
        IMiddleware::{
            self, IMiddlewareInstance, InitParams as MiddlewareInitParams,
            initializeCall as MiddlewareInitializeCall,
        },
        IMirror,
        IRouter::{self, IRouterInstance, initializeCall as RouterInitializeCall},
        ITransparentUpgradeableProxy,
        IWrappedVara::{self, IWrappedVaraInstance, initializeCall as WrappedVaraInitializeCall},
        middleware_abi::Gear::SymbioticContracts,
        symbiotic_abi::*,
    },
    create_provider,
};
use alloy::{
    primitives::{Address, Bytes, U256, Uint},
    providers::{Provider, WalletProvider},
    sol_types::SolCall,
};
use anyhow::Result;
use ethexe_common::{
    Address as LocalAddress, ValidatorsVec, ecdsa::PublicKey, gear::AggregatedPublicKey,
};
use gprimitives::{ActorId, H160, U256 as GearU256};
use gsigner::secp256k1::Signer as LocalSigner;
use roast_secp256k1_evm::frost::{
    Identifier,
    keys::{self, IdentifierList, PublicKeyPackage, VerifiableSecretSharingCommitment},
};
use std::{collections::BTreeSet, convert::TryInto};

/// The offset for mirror address calculation in router deployment.
const MIRROR_DEPLOYMENT_NONCE_OFFSET: u64 = 2;

/// The offset for middleware address calculation in router deployment.
/// Offset equals `16` because of a symbiotic deployment contracts.
const MIDDLEWARE_DEPLOYMENT_NONCE_OFFSET: u64 = 16;

/// [`EthereumDeployer`] is a builder for deploying smart contracts on Ethereum for testing purposes.
pub struct EthereumDeployer {
    // Required parameters
    provider: AlloyProvider,

    // Customizable parameters
    /// Validators`s addresses. If not provided, will use as vec with one element.
    validators: ValidatorsVec,

    /// Customizable deployment parameters for smart contracts.
    params: ContractsDeploymentParams,

    /// Verifiable secret sharing commitment generated during key generation.
    /// If not provided, will be generate with [`keys::generate_with_dealer`] function.
    verifiable_secret_sharing_commitment: Option<VerifiableSecretSharingCommitment>,
}

#[derive(Debug, Copy, Clone)]
pub struct ContractsDeploymentParams {
    // whether to deploy middleware contract
    pub with_middleware: bool,

    // customizable timelines
    pub era_duration: u64,
    pub election_duration: u64,
}

#[derive(Debug)]
pub struct SymbioticOperatorConfig {
    pub stake: U256,
}

impl Default for ContractsDeploymentParams {
    fn default() -> Self {
        Self {
            with_middleware: true,
            era_duration: 24 * 60 * 60,     // 1 day
            election_duration: 2 * 60 * 60, // 2 hours
        }
    }
}

// Public methods
impl EthereumDeployer {
    /// Creates a new deployer from necessary arguments.
    pub async fn new(rpc: &str, signer: LocalSigner, sender_address: LocalAddress) -> Result<Self> {
        let provider = create_provider(rpc, signer, sender_address).await?;
        Ok(EthereumDeployer {
            provider,
            validators: nonempty::nonempty![LocalAddress([1u8; 20])].into(),
            params: Default::default(),
            verifiable_secret_sharing_commitment: None,
        })
    }

    pub fn with_middleware(mut self) -> Self {
        self.params.with_middleware = true;
        self
    }

    pub fn with_era_duration(mut self, era_duration: u64) -> Self {
        self.params.era_duration = era_duration;
        self
    }

    pub fn with_election_duration(mut self, election_duration: u64) -> Self {
        self.params.election_duration = election_duration;
        self
    }

    pub fn with_params(mut self, new_params: ContractsDeploymentParams) -> Self {
        self.params = new_params;
        self
    }

    pub fn with_validators(mut self, validators: ValidatorsVec) -> Self {
        self.validators = validators;
        self
    }

    pub fn with_verifiable_secret_sharing_commitment(
        mut self,
        verifiable_secret_sharing_commitment: VerifiableSecretSharingCommitment,
    ) -> Self {
        self.verifiable_secret_sharing_commitment = Some(verifiable_secret_sharing_commitment);
        self
    }

    pub async fn deploy(self) -> Result<Ethereum> {
        let router = self.deploy_contracts().await?;
        Ethereum::from_provider(self.provider.clone(), router).await
    }
}

// Private implementation details
impl EthereumDeployer {
    /// Deploy all contracts and return the router address.
    async fn deploy_contracts(&self) -> Result<Address> {
        let deployer = self.provider.default_signer_address();
        let wrapped_vara = deploy_wrapped_vara(deployer, self.provider.clone()).await?;

        // NOTE: The order of deployment is important here because of the future addresses calculation
        // inside `deploy_router`.
        let nonce = self.provider.get_transaction_count(deployer).await?;

        let router = deploy_router(
            deployer,
            *wrapped_vara.address(),
            self.verifiable_secret_sharing_commitment.clone(),
            self.validators
                .clone()
                .into_iter()
                .map(Into::into)
                .collect(),
            self.params,
            self.provider.clone(),
        )
        .await?;

        let mirror = IMirror::deploy(self.provider.clone(), *router.address()).await?;
        log::debug!("Mirror impl has been deployed at {}", mirror.address());

        debug_assert_eq!(
            self.provider.get_transaction_count(deployer).await?,
            nonce + MIRROR_DEPLOYMENT_NONCE_OFFSET + 1,
            "Nonce mismatch. Check the tx count and deployment order in deploy_router()."
        );

        if self.params.with_middleware {
            let _ =
                deploy_middleware(deployer, &router, &wrapped_vara, self.provider.clone()).await?;

            debug_assert_eq!(
                self.provider.get_transaction_count(deployer).await?,
                nonce + MIDDLEWARE_DEPLOYMENT_NONCE_OFFSET + 1,
                "Nonce mismatch. Check the tx count and deployment order in deploy_middleware()."
            );
        }

        assert_eq!(router.mirrorImpl().call().await?, *mirror.address());

        let builder = router.lookupGenesisHash();
        builder.send().await?.try_get_receipt().await?;

        Ok(*router.address())
    }
}

async fn deploy_wrapped_vara<P>(deployer: Address, provider: P) -> Result<IWrappedVaraInstance<P>>
where
    P: Provider + Clone,
{
    let wrapped_vara_impl = IWrappedVara::deploy(provider.clone()).await?;
    let proxy = ITransparentUpgradeableProxy::deploy(
        provider.clone(),
        *wrapped_vara_impl.address(),
        deployer,
        Bytes::copy_from_slice(
            &WrappedVaraInitializeCall {
                initialOwner: deployer,
            }
            .abi_encode(),
        ),
    )
    .await?;

    let wrapped_vara = IWrappedVara::new(*proxy.address(), provider);

    log::debug!(
        "WrappedVara impl has been deployed at {}",
        wrapped_vara_impl.address()
    );
    log::debug!("WrappedVara deployed at {}", wrapped_vara.address());

    Ok(wrapped_vara)
}

async fn deploy_router<P>(
    deployer: Address,
    wvara_address: Address,
    maybe_verifiable_secret_sharing_commitment: Option<VerifiableSecretSharingCommitment>,
    validators: Vec<Address>,
    params: ContractsDeploymentParams,
    provider: P,
) -> Result<IRouterInstance<P>>
where
    P: Provider + Clone,
{
    let validators_identifiers: Vec<_> = validators
        .iter()
        .map(|address| {
            Identifier::deserialize(&ActorId::from(H160(address.0.0)).into_bytes())
                .expect("conversion failed")
        })
        .collect();

    let verifiable_secret_sharing_commitment = match maybe_verifiable_secret_sharing_commitment {
        Some(commitment) => commitment,
        None => generate_secret_sharing_commitment(&validators_identifiers),
    };
    let identifiers = validators_identifiers.clone().into_iter().collect();
    let aggregated_public_key =
        aggregated_public_key(&identifiers, &verifiable_secret_sharing_commitment);

    // Calculate future contracts addresses for mirror and middleware
    let nonce = provider.get_transaction_count(deployer).await?;
    let mirror_address = deployer.create(
        nonce
            .checked_add(MIRROR_DEPLOYMENT_NONCE_OFFSET)
            .expect("nonce overflow when deploying router with mirror"),
    );

    let middleware_address = match params.with_middleware {
        true => deployer.create(
            // Add 12 to nonce because of deployment symbiotic contracts
            nonce
                .checked_add(MIDDLEWARE_DEPLOYMENT_NONCE_OFFSET)
                .expect("nonce overflow when deploying middleware"),
        ),
        false => Address::default(),
    };

    let router_impl = IRouter::deploy(provider.clone()).await?;
    let proxy = ITransparentUpgradeableProxy::deploy(
        provider.clone(),
        *router_impl.address(),
        deployer,
        Bytes::copy_from_slice(
            &RouterInitializeCall {
                _owner: deployer,
                _mirror: mirror_address,
                _wrappedVara: wvara_address,
                _middleware: middleware_address,
                _eraDuration: U256::from(params.era_duration),
                _electionDuration: U256::from(params.election_duration),
                _validationDelay: U256::from(1),
                _aggregatedPublicKey: (aggregated_public_key).into(),
                _verifiableSecretSharingCommitment: Bytes::copy_from_slice(
                    &verifiable_secret_sharing_commitment.serialize()?.concat(),
                ),
                _validators: validators,
            }
            .abi_encode(),
        ),
    )
    .await?;

    let router_address = *proxy.address();
    let router = IRouter::new(router_address, provider.clone());

    log::debug!("Router impl has been deployed at {}", router_impl.address());
    log::debug!("Router proxy has been deployed at {}", router.address());

    Ok(router)
}

async fn deploy_middleware<P>(
    deployer: Address,
    router: &IRouterInstance<P>,
    wrapped_vara: &IWrappedVaraInstance<P>,
    provider: P,
) -> Result<IMiddlewareInstance<P>>
where
    P: Provider + Clone,
{
    let middleware_impl = IMiddleware::deploy(provider.clone()).await?;

    // Deploy Symbiotic contracts
    let vault_factory = VaultFactory::deploy(provider.clone(), deployer).await?;
    let delegator_factory = DelegatorFactory::deploy(provider.clone(), deployer).await?;
    let slasher_factory = SlasherFactory::deploy(provider.clone(), deployer).await?;
    let operator_registry = OperatorRegistry::deploy(provider.clone()).await?;
    let network_registry = NetworkRegistry::deploy(provider.clone()).await?;
    let network_middleware_service =
        NetworkMiddlewareService::deploy(provider.clone(), *network_registry.address()).await?;
    let network_opt_in = OptInService::deploy(
        provider.clone(),
        *operator_registry.address(),
        *network_registry.address(),
        "Network Opt-In Service".to_string(),
    )
    .await?;

    // Whitelisting deployed contracts in registries
    let staker_rewards_impl = DefaultStakerRewards::deploy(
        provider.clone(),
        *vault_factory.address(),
        *network_middleware_service.address(),
    )
    .await?;

    let staker_rewards_factory =
        DefaultStakerRewardsFactory::deploy(provider.clone(), *staker_rewards_impl.address())
            .await?;
    let operator_rewards =
        DefaultOperatorRewards::deploy(provider.clone(), *network_middleware_service.address())
            .await?;

    let vault_impl = Vault::deploy(
        provider.clone(),
        *delegator_factory.address(),
        *slasher_factory.address(),
        *vault_factory.address(),
    )
    .await?;

    let _receipt = vault_factory
        .whitelist(*vault_impl.address())
        .send()
        .await
        .unwrap()
        .try_get_receipt()
        .await
        .unwrap();

    // Prepare initialization parameters for middleware
    let symbiotic = SymbioticContracts {
        vaultRegistry: *vault_factory.address(),
        operatorRegistry: *operator_registry.address(),
        networkRegistry: *network_registry.address(),
        middlewareService: *network_middleware_service.address(),
        networkOptIn: *network_opt_in.address(),
        stakerRewardsFactory: *staker_rewards_factory.address(),

        operatorRewards: *operator_rewards.address(),
        roleSlashRequester: *router.address(),
        roleSlashExecutor: *router.address(),
        vetoResolver: *router.address(),
    };

    let middleware_init_params = MiddlewareInitParams {
        owner: deployer,
        eraDuration: Uint::<48, 1>::from(24 * 60 * 60),
        minVaultEpochDuration: Uint::<48, 1>::from(2 * 24 * 60 * 60), // 2 eras
        operatorGracePeriod: Uint::<48, 1>::from(7 * 24 * 60 * 60),
        vaultGracePeriod: Uint::from(2 * 24 * 60 * 60), // 2 eras
        minVetoDuration: Uint::from(2 * 60 * 60),       // 2 h
        minSlashExecutionDelay: Uint::from(5 * 60),     // 5 min
        allowedVaultImplVersion: vault_factory.lastVersion().call().await?,

        // TODO (kuzmin-dev): remove this constant and use slasher type from slasher factory
        // TODO (kuzmin-dev): add delegator type also (means that we will support only one type of delegator)
        vetoSlasherImplType: 0,
        maxResolverSetEpochsDelay: Uint::from(5 * 60), // 5 min
        collateral: *wrapped_vara.address(),
        maxAdminFee: Uint::from(3), // 3%
        router: *router.address(),
        symbiotic,
    };

    let proxy = ITransparentUpgradeableProxy::deploy(
        provider.clone(),
        *middleware_impl.address(),
        deployer,
        Bytes::copy_from_slice(
            &MiddlewareInitializeCall {
                _params: (middleware_init_params),
            }
            .abi_encode(),
        ),
    )
    .await?;

    let middleware = IMiddleware::new(*proxy.address(), provider.clone());
    log::debug!(
        "Middleware impl has been deployed at {}",
        middleware_impl.address()
    );
    log::debug!("Middleware proxy deployed at {}", middleware.address());

    Ok(middleware)
}

fn generate_secret_sharing_commitment(
    identifiers: &[Identifier],
) -> VerifiableSecretSharingCommitment {
    let (mut secret_shares, _) = keys::generate_with_dealer(
        1,
        1,
        IdentifierList::Custom(identifiers),
        rand::thread_rng(),
    )
    .unwrap();

    secret_shares
        .pop_first()
        .map(|(_, share)| share.commitment().clone())
        .unwrap()
}

fn aggregated_public_key(
    identifiers: &BTreeSet<Identifier>,
    verifiable_secret_sharing_commitment: &VerifiableSecretSharingCommitment,
) -> AggregatedPublicKey {
    let public_key_package =
        PublicKeyPackage::from_commitment(identifiers, verifiable_secret_sharing_commitment)
            .expect("conversion failed");
    let public_key_compressed: [u8; 33] = public_key_package
        .verifying_key()
        .serialize()
        .expect("conversion failed")
        .try_into()
        .unwrap();
    let public_key_uncompressed = PublicKey::from_bytes(public_key_compressed)
        .expect("verifying key produces valid compressed bytes")
        .to_uncompressed();
    let (public_key_x_bytes, public_key_y_bytes) = public_key_uncompressed.split_at(32);

    AggregatedPublicKey {
        x: GearU256::from_big_endian(public_key_x_bytes),
        y: GearU256::from_big_endian(public_key_y_bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use alloy::node_bindings::Anvil;

    #[tokio::test]
    async fn test_deployment_with_middleware() -> Result<()> {
        gear_utils::init_default_logger();

        let anvil = Anvil::new().block_time_f64(0.1).try_spawn()?;
        let signer = LocalSigner::memory();

        let sender_public_key = signer.storage_mut().add_key(
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".parse()?,
        )?;
        let sender_address = sender_public_key.to_address();

        let ethereum = EthereumDeployer::new(&anvil.endpoint(), signer, sender_address)
            .await?
            .with_middleware()
            .deploy()
            .await?;

        let router = ethereum.router();
        let middleware = ethereum.middleware();

        assert_eq!(
            middleware.query().router().await?,
            router.address(),
            "Router address mismatch"
        );

        Ok(())
    }
}
