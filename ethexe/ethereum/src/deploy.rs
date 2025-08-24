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
        IMirror::{self, IMirrorInstance},
        IRouter::{self, IRouterInstance, initializeCall as RouterInitializeCall},
        ITransparentUpgradeableProxy,
        IWrappedVara::{self, IWrappedVaraInstance, initializeCall as WrappedVaraInitializeCall},
    },
    create_provider,
    mirror::Mirror,
    router::{Router, RouterQuery},
};
use alloy::{
    consensus::SignableTransaction,
    network::{Ethereum as AlloyEthereum, EthereumWallet, Network, TxSigner},
    primitives::{Address, B256, Bytes, ChainId, Signature, SignatureError, U256, bytes::Buf},
    providers::{
        Identity, PendingTransactionBuilder, PendingTransactionError, Provider, ProviderBuilder,
        RootProvider,
        fillers::{
            BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller,
            SimpleNonceManager, WalletFiller,
        },
    },
    rpc::types::eth::Log,
    signers::{
        self as alloy_signer, Error as SignerError, Result as SignerResult, Signer, SignerSync,
        sign_transaction_with_chain_id,
    },
    sol_types::{SolCall, SolEvent},
    transports::RpcError,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use ethexe_common::{Address as LocalAddress, Digest, ecdsa::PublicKey, gear::AggregatedPublicKey};
use ethexe_signer::Signer as LocalSigner;
use gprimitives::{ActorId, U256 as GearU256};
use roast_secp256k1_evm::frost::{
    Identifier,
    keys::{PublicKeyPackage, VerifiableSecretSharingCommitment},
};
use std::time::Duration;

/// Deployer for Gear.exe contracts.
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

    let instance = IWrappedVara::new(*proxy.address(), provider);

    log::debug!(
        "WrappedVara impl has been deployed at {}",
        wrapped_vara_impl.address()
    );
    log::debug!("WrappedVara deployed at {}", instance.address());

    Ok(instance)
}

async fn deploy_router_with_mirror<P>(
    deployer: Address,
    wvara_address: Address,
    aggregated_public_key: AggregatedPublicKey,
    verifiable_secret_sharing_commitment: VerifiableSecretSharingCommitment,
    validators: Vec<Address>,
    provider: P,
) -> Result<(IRouterInstance<P>, IMirrorInstance<P>)>
where
    P: Provider + Clone,
{
    let nonce = provider.get_transaction_count(deployer).await?;
    let mirror_address = deployer.create(
        nonce
            .checked_add(2)
            .expect("nonce overflow when deploying router with mirror"),
    );

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
                _middleware: Address::ZERO,
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

async fn deploy_middleware<P>(deployer: Address, router: IRouterInstance<P>, provider: P) -> Result<IMiddlewareInstance<P>>
where
    P: Provider + Clone,
{
    let nonce = provider.get_transaction_count(deployer).await?;
    let middleware_impl = IMiddleware::deploy(provider.clone()).await?;

    let middleware_init_params = MiddlewareInitParams {
                owner: deployer,
                eraDuration: Uint<48, 1>::from(24 * 60 * 60),
                minVaultEpochDuration: Uint<48, 1>::from(2 * 60 * 60),
                operatorGracePeriod: Uint<48, 1>::from(7 * 24 * 60 * 60),
                vaultGracePeriod
                minVetoDuration
                minSlashExecutionDelay
                allowedVaultImplVersion
                vetoSlasherImplType
                maxResolverSetEpochsDelay
                collateral
                maxAdminFee
                operatorRewards
                router
                roleSlashRequester
                roleSlashExecutor
                vetoResolver
                registries
    };
    let proxy = ITransparentUpgradeableProxy::deploy(
        provider.clone(),
        *middleware_impl.address(),
        deployer,
        Bytes::copy_from_slice(MiddlewareInitializeCall {
            _params: 
        }.abi_encode()),
    );

    unimplemented!()
}

impl EthereumDeployer<'_> {
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
        let deployer_address = Address::new(self.sender_address.0);

        let wrapped_vara = deploy_wrapped_vara(deployer_address, provider.clone()).await?;
        let (router, mirror) = deploy_router_with_mirror(
            deployer_address,
            *wrapped_vara.address(),
            AggregatedPublicKey {
                x: GearU256::from_big_endian(public_key_x_bytes),
                y: GearU256::from_big_endian(public_key_y_bytes),
            },
            self.verifiable_secret_sharing_commitment,
            validators,
            provider.clone(),
        )
        .await?;

        let builder = wrapped_vara.approve(*router.address(), U256::MAX);
        builder.send().await?.try_get_receipt().await?;

        assert_eq!(router.mirrorImpl().call().await?, *mirror.address());

        let builder = router.lookupGenesisHash();
        builder.send().await?.try_get_receipt().await?;

        Ok(Ethereum {
            router_address: *router.address(),
            wvara_address: *wrapped_vara.address(),
            provider,
        })
    }
}

#[test]
async fn test_deployment_correctness() -> Result<()> {
    unimplemented!()
}
