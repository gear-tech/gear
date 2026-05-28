// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    Ethereum, EthereumBuilder, TryGetReceipt,
    abi::{
        IERC1967Proxy,
        IMiddleware::{
            self, IMiddlewareInstance, InitParams as MiddlewareInitParams,
            initializeCall as MiddlewareInitializeCall,
        },
        IMirror,
        IRouter::{self, IRouterInstance, initializeCall as RouterInitializeCall},
        IWrappedVara::{self, IWrappedVaraInstance, initializeCall as WrappedVaraInitializeCall},
        middleware_abi::Gear::SymbioticContracts,
        symbiotic_abi::*,
    },
};
use alloy::{
    primitives::{Address, Bytes, U256, Uint},
    providers::{Provider, WalletProvider},
    sol_types::SolCall,
};
use anyhow::Result;
use ethexe_common::{Address as LocalAddress, ValidatorsVec, gear::AggregatedPublicKey};
use gprimitives::U256 as GearU256;
use gsigner::secp256k1::Signer as LocalSigner;

/// The offset for mirror address calculation in router deployment.
const MIRROR_DEPLOYMENT_NONCE_OFFSET: u64 = 2;

/// The offset for middleware address calculation in router deployment.
/// Offset equals `16` because of a symbiotic deployment contracts.
const MIDDLEWARE_DEPLOYMENT_NONCE_OFFSET: u64 = 16;

/// [`EthereumDeployer`] is a builder for deploying smart contracts on Ethereum for testing purposes.
pub struct EthereumDeployer {
    // Required parameters
    ethereum: Ethereum,

    // Customizable parameters
    /// Validators`s addresses. If not provided, will use as vec with one element.
    validators: ValidatorsVec,

    /// Customizable deployment parameters for smart contracts.
    params: ContractsDeploymentParams,

    /// Serialized verifiable secret sharing commitment generated during key generation.
    /// If not provided, an empty commitment is passed to the router.
    verifiable_secret_sharing_commitment: Option<Vec<u8>>,
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
    /// Hack to pre-compute router address before deployment and improve architecture.
    /// This is used to create [`Ethereum`] instance with correct router address.
    const ROUTER_ADDRESS_OFFSET: u64 = 3;

    /// Creates a new deployer from necessary arguments.
    pub async fn new(rpc: &str, signer: LocalSigner, sender_address: LocalAddress) -> Result<Self> {
        let alloy_sender_address: Address = sender_address.into();
        let ethereum = EthereumBuilder::default()
            .rpc_url(rpc)
            .router_address(
                alloy_sender_address
                    .create(Self::ROUTER_ADDRESS_OFFSET)
                    .into(),
            )
            .signer(signer)
            .sender_address(sender_address)
            .without_initializing_addresses()
            .build()
            .await?;
        Ok(EthereumDeployer {
            ethereum,
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
        verifiable_secret_sharing_commitment: Vec<u8>,
    ) -> Self {
        self.verifiable_secret_sharing_commitment = Some(verifiable_secret_sharing_commitment);
        self
    }

    pub async fn deploy(mut self) -> Result<Ethereum> {
        self.deploy_contracts().await?;
        self.ethereum.initialize_addresses().await?;
        Ok(self.ethereum)
    }
}

// Private implementation details
impl EthereumDeployer {
    /// Deploy all contracts and return the router address.
    async fn deploy_contracts(&self) -> Result<Address> {
        let provider = self.ethereum.provider().clone();
        let deployer = provider.default_signer_address();
        let wrapped_vara = deploy_wrapped_vara(deployer, provider.clone()).await?;

        // NOTE: The order of deployment is important here because of the future addresses calculation
        // inside `deploy_router`.
        let nonce = provider.get_transaction_count(deployer).await?;

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
            provider.clone(),
        )
        .await?;

        let mirror = IMirror::deploy(provider.clone(), *router.address()).await?;
        log::debug!("Mirror impl has been deployed at {}", mirror.address());

        debug_assert_eq!(
            self.ethereum
                .provider()
                .get_transaction_count(deployer)
                .await?,
            nonce + MIRROR_DEPLOYMENT_NONCE_OFFSET + 1,
            "Nonce mismatch. Check the tx count and deployment order in deploy_router()."
        );

        if self.params.with_middleware {
            let _ = deploy_middleware(deployer, &router, &wrapped_vara, provider.clone()).await?;

            debug_assert_eq!(
                self.ethereum
                    .provider()
                    .get_transaction_count(deployer)
                    .await?,
                nonce + MIDDLEWARE_DEPLOYMENT_NONCE_OFFSET + 1,
                "Nonce mismatch. Check the tx count and deployment order in deploy_middleware()."
            );
        }

        assert_eq!(router.mirrorImpl().call().await?, *mirror.address());

        let builder = router.lookupGenesisHash();
        builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;

        Ok(*router.address())
    }
}

async fn deploy_wrapped_vara<P>(deployer: Address, provider: P) -> Result<IWrappedVaraInstance<P>>
where
    P: Provider + Clone,
{
    let wrapped_vara_impl = IWrappedVara::deploy(provider.clone()).await?;
    let proxy = IERC1967Proxy::deploy(
        provider.clone(),
        *wrapped_vara_impl.address(),
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
    maybe_verifiable_secret_sharing_commitment: Option<Vec<u8>>,
    validators: Vec<Address>,
    params: ContractsDeploymentParams,
    provider: P,
) -> Result<IRouterInstance<P>>
where
    P: Provider + Clone,
{
    let verifiable_secret_sharing_commitment =
        maybe_verifiable_secret_sharing_commitment.unwrap_or_default();
    let aggregated_public_key = placeholder_aggregated_public_key();

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
    let proxy = IERC1967Proxy::deploy(
        provider.clone(),
        *router_impl.address(),
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
                    &verifiable_secret_sharing_commitment,
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
        .try_get_receipt_check_reverted()
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

    let proxy = IERC1967Proxy::deploy(
        provider.clone(),
        *middleware_impl.address(),
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

fn placeholder_aggregated_public_key() -> AggregatedPublicKey {
    const SECP256K1_GENERATOR_X: [u8; 32] = [
        0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0, 0x62, 0x95, 0xce, 0x87, 0x0b,
        0x07, 0x02, 0x9b, 0xfc, 0xdb, 0x2d, 0xce, 0x28, 0xd9, 0x59, 0xf2, 0x81, 0x5b, 0x16, 0xf8,
        0x17, 0x98,
    ];
    const SECP256K1_GENERATOR_Y: [u8; 32] = [
        0x48, 0x3a, 0xda, 0x77, 0x26, 0xa3, 0xc4, 0x65, 0x5d, 0xa4, 0xfb, 0xfc, 0x0e, 0x11, 0x08,
        0xa8, 0xfd, 0x17, 0xb4, 0x48, 0xa6, 0x85, 0x54, 0x19, 0x9c, 0x47, 0xd0, 0x8f, 0xfb, 0x10,
        0xd4, 0xb8,
    ];

    AggregatedPublicKey {
        x: GearU256::from_big_endian(&SECP256K1_GENERATOR_X),
        y: GearU256::from_big_endian(&SECP256K1_GENERATOR_Y),
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

        let sender_public_key = signer.import(
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
