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
    Ethereum, TryGetReceipt,
    abi::{
        IMiddleware::{
            self, IMiddlewareInstance, InitParams as MiddlewareInitParams,
            initializeCall as MiddlewareInitializeCall,
        },
        IMirror::{self, IMirrorInstance},
        IRouter::{self, IRouterInstance, initializeCall as RouterInitializeCall},
        ITransparentUpgradeableProxy,
        IWrappedVara::{self, IWrappedVaraInstance, initializeCall as WrappedVaraInitializeCall},
        middleware_abi::Gear::SymbioticRegistries,
        symbiotic_abi::*,
    },
    create_provider, 
};
use alloy::{
    primitives::{Address, Bytes, U256, Uint},
    providers::Provider,
    sol_types::SolCall,
};
use anyhow::Result;
use ethexe_common::{Address as LocalAddress, ecdsa::PublicKey, gear::AggregatedPublicKey};
use ethexe_signer::Signer as LocalSigner;
use gprimitives::{ActorId, U256 as GearU256};
use roast_secp256k1_evm::frost::{
    Identifier,
    keys::{PublicKeyPackage, VerifiableSecretSharingCommitment},
};

/// Smart contracts deployer for testing purposes.
pub struct EthereumDeployer<'a> {
    rpc_url: &'a str,
    signer: LocalSigner,
    sender_address: LocalAddress,
    verifiable_secret_sharing_commitment: VerifiableSecretSharingCommitment,
    validators: Vec<LocalAddress>,
    with_middleware: bool,
}

impl<'a> EthereumDeployer<'a> {
    /// Creates a new deployer from necessary arguments.
    pub fn new(
        rpc_url: &'a str,
        signer: LocalSigner,
        sender_address: LocalAddress,
        verifiable_secret_sharing_commitment: VerifiableSecretSharingCommitment,
    ) -> Self {
        Self {
            rpc_url,
            signer,
            verifiable_secret_sharing_commitment,
            sender_address,
            with_middleware: false,
            validators: vec![],
        }
    }
}

impl<'a> EthereumDeployer<'a> {
    pub fn with_validators(mut self, validators: Vec<LocalAddress>) -> Self {
        self.validators = validators;
        self
    }

    pub fn with_middleware(mut self) -> Self {
        self.with_middleware = true;
        self
    }

    pub async fn deploy(self) -> Result<Ethereum> {
        let maybe_validator_identifiers: Result<Vec<_>, _> = self
            .validators
            .iter()
            .map(|address| Identifier::deserialize(&ActorId::from(*address).into_bytes()))
            .collect();
        let validator_identifiers = maybe_validator_identifiers?;
        let identifiers = validator_identifiers.into_iter().collect();
        let public_key_package = PublicKeyPackage::from_commitment(
            &identifiers,
            &self.verifiable_secret_sharing_commitment,
        )?;
        let public_key_compressed: [u8; 33] = public_key_package
            .verifying_key()
            .serialize()?
            .try_into()
            .unwrap();
        let public_key_uncompressed = PublicKey(public_key_compressed).to_uncompressed();
        let (public_key_x_bytes, public_key_y_bytes) = public_key_uncompressed.split_at(32);

        let provider = create_provider(self.rpc_url, self.signer, self.sender_address).await?;
        let validators: Vec<_> = self
            .validators
            .into_iter()
            .map(|validator_address| Address::new(validator_address.0))
            .collect();
        let deployer = Address::new(self.sender_address.0);

        let nonce = provider.get_transaction_count(deployer).await?;
        let mirror_address = deployer.create(
            nonce
                .checked_add(2)
                .expect("nonce overflow when deploying router with mirror"),
        );

        let middleware_address = match self.with_middleware {
            true => deployer.create(
                nonce
                    .checked_add(3)
                    .expect("nonce overflow when deploying middleware"),
            ),
            false => Address::ZERO,
        };

        let aggregated_public_key = AggregatedPublicKey {
            x: GearU256::from_big_endian(public_key_x_bytes),
            y: GearU256::from_big_endian(public_key_y_bytes),
        };
        let wrapped_vara = deploy_wrapped_vara(deployer, provider.clone()).await?;
        let (router, mirror) = deploy_router_with_mirror(
            deployer,
            *wrapped_vara.address(),
            mirror_address,
            middleware_address,
            aggregated_public_key,
            self.verifiable_secret_sharing_commitment,
            validators,
            provider.clone(),
        )
        .await?;

        let _middleware = if self.with_middleware {
            Some(deploy_middleware(deployer, &router, &wrapped_vara, provider.clone()).await?)
        } else {
            None
        };

        let builder = wrapped_vara.approve(*router.address(), U256::MAX);
        builder.send().await?.try_get_receipt().await?;

        assert_eq!(router.mirrorImpl().call().await?, *mirror.address());

        let builder = router.lookupGenesisHash();
        builder.send().await?.try_get_receipt().await?;

        Ok(Ethereum {
            router_address: *router.address(),
            wvara_address: *wrapped_vara.address(),
            middleware_address: None,
            provider,
        })
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

async fn deploy_router_with_mirror<P>(
    deployer: Address,
    wvara_address: Address,
    mirror_address: Address,
    middleware_address: Address,
    aggregated_public_key: AggregatedPublicKey,
    verifiable_secret_sharing_commitment: VerifiableSecretSharingCommitment,
    validators: Vec<Address>,
    provider: P,
) -> Result<(IRouterInstance<P>, IMirrorInstance<P>)>
where
    P: Provider + Clone,
{
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
                _eraDuration: U256::from(24 * 60 * 60),
                _electionDuration: U256::from(2 * 60 * 60),
                _validationDelay: U256::from(60),
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
    let mirror = IMirror::deploy(provider.clone(), router_address).await?;

    log::debug!("Mirror impl has been deployed at {}", mirror.address());
    log::debug!("Router impl has been deployed at {}", router_impl.address());
    log::debug!("Router proxy has been deployed at {}", router.address());

    Ok((router, mirror))
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

    // Prepare initialization parameters for middleware
    let registries = SymbioticRegistries {
        vaultRegistry: *vault_factory.address(),
        operatorRegistry: *operator_registry.address(),
        networkRegistry: *network_registry.address(),
        middlewareService: *network_middleware_service.address(),
        networkOptIn: *network_opt_in.address(),
        stakerRewardsFactory: *staker_rewards_factory.address(),
    };

    let middleware_init_params = MiddlewareInitParams {
        owner: deployer,
        eraDuration: Uint::<48, 1>::from(24 * 60 * 60),
        minVaultEpochDuration: Uint::<48, 1>::from(2 * 60 * 60),
        operatorGracePeriod: Uint::<48, 1>::from(7 * 24 * 60 * 60),
        vaultGracePeriod: Uint::from(0),
        minVetoDuration: Uint::from(0),
        minSlashExecutionDelay: Uint::from(0),
        allowedVaultImplVersion: vault_factory.lastVersion().call().await?,
        vetoSlasherImplType: 0,
        maxResolverSetEpochsDelay: Uint::from(0),
        collateral: *wrapped_vara.address(),
        maxAdminFee: Uint::from(3), // 3%
        operatorRewards: *operator_rewards.address(),
        router: *router.address(),
        roleSlashRequester: Address::ZERO,
        roleSlashExecutor: Address::ZERO,
        vetoResolver: Address::ZERO,
        registries,
    };

    let proxy = ITransparentUpgradeableProxy::deploy(
        provider.clone(),
        *middleware_impl.address(),
        deployer,
        Bytes::copy_from_slice(
            &MiddlewareInitializeCall {
                _params: middleware_init_params,
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
