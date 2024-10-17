// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

#![allow(dead_code, clippy::new_without_default)]

use abi::{
    IMirror, IMirrorProxy,
    IRouter::{self, initializeCall as RouterInitializeCall},
    ITransparentUpgradeableProxy,
    IWrappedVara::{self, initializeCall as WrappedVaraInitializeCall},
};
use alloy::{
    consensus::{self as alloy_consensus, SignableTransaction},
    network::{Ethereum as AlloyEthereum, EthereumWallet, Network, TxSigner},
    primitives::{Address, Bytes, ChainId, Signature, B256, U256},
    providers::{
        fillers::{
            BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller,
            WalletFiller,
        },
        Identity, PendingTransactionBuilder, PendingTransactionError, Provider, ProviderBuilder,
        RootProvider,
    },
    rpc::types::eth::Log,
    signers::{
        self as alloy_signer, sign_transaction_with_chain_id, Error as SignerError,
        Result as SignerResult, Signer, SignerSync,
    },
    sol_types::{SolCall, SolEvent},
    transports::{BoxTransport, RpcError, Transport},
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use ethexe_signer::{Address as LocalAddress, PublicKey, Signer as LocalSigner};
use mirror::Mirror;
use router::{Router, RouterQuery};
use std::{sync::Arc, time::Duration};

mod abi;
mod eip1167;
pub mod mirror;
pub mod router;
pub mod wvara;

pub(crate) type AlloyTransport = BoxTransport;
type AlloyProvider =
    FillProvider<ExeFiller, RootProvider<AlloyTransport>, AlloyTransport, AlloyEthereum>;

pub(crate) type ExeFiller = JoinFill<
    JoinFill<
        Identity,
        JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>,
    >,
    WalletFiller<EthereumWallet>,
>;

pub struct Ethereum {
    router_address: Address,
    wvara_address: Address,
    provider: Arc<AlloyProvider>,
}

impl Ethereum {
    pub async fn new(
        rpc_url: &str,
        router_address: LocalAddress,
        signer: LocalSigner,
        sender_address: LocalAddress,
    ) -> Result<Self> {
        let router_query = RouterQuery::new(rpc_url, router_address).await?;
        let wvara_address = router_query.wvara_address().await?;

        let router_address = Address::new(router_address.0);

        let provider = create_provider(rpc_url, signer, sender_address).await?;

        Ok(Self {
            router_address,
            wvara_address,
            provider,
        })
    }

    pub async fn deploy(
        rpc_url: &str,
        validators: Vec<LocalAddress>,
        signer: LocalSigner,
        sender_address: LocalAddress,
    ) -> Result<Self> {
        const VALUE_PER_GAS: u128 = 6;

        let provider = create_provider(rpc_url, signer, sender_address).await?;
        let validators = validators
            .into_iter()
            .map(|validator_address| Address::new(validator_address.0))
            .collect();
        let deployer_address = Address::new(sender_address.0);

        let wrapped_vara_impl = IWrappedVara::deploy(provider.clone()).await?;
        let proxy = ITransparentUpgradeableProxy::deploy(
            provider.clone(),
            *wrapped_vara_impl.address(),
            deployer_address,
            Bytes::copy_from_slice(
                &WrappedVaraInitializeCall {
                    initialOwner: deployer_address,
                }
                .abi_encode(),
            ),
        )
        .await?;
        let wrapped_vara = IWrappedVara::new(*proxy.address(), provider.clone());
        let wvara_address = *wrapped_vara.address();

        let nonce = provider.get_transaction_count(deployer_address).await?;
        let mirror_address = deployer_address.create(
            nonce
                .checked_add(2)
                .ok_or_else(|| anyhow!("failed to add 2"))?,
        );
        let mirror_proxy_address = deployer_address.create(
            nonce
                .checked_add(3)
                .ok_or_else(|| anyhow!("failed to add 3"))?,
        );

        let router_impl = IRouter::deploy(provider.clone()).await?;
        let proxy = ITransparentUpgradeableProxy::deploy(
            provider.clone(),
            *router_impl.address(),
            deployer_address,
            Bytes::copy_from_slice(
                &RouterInitializeCall {
                    initialOwner: deployer_address,
                    _mirror: mirror_address,
                    _mirrorProxy: mirror_proxy_address,
                    _wrappedVara: wvara_address,
                    _validatorsKeys: validators,
                }
                .abi_encode(),
            ),
        )
        .await?;
        let router_address = *proxy.address();
        let router = IRouter::new(router_address, provider.clone());

        let mirror = IMirror::deploy(provider.clone()).await?;
        let mirror_proxy = IMirrorProxy::deploy(provider.clone(), router_address).await?;

        let builder = wrapped_vara.approve(router_address, U256::MAX);
        builder.send().await?.try_get_receipt().await?;

        assert_eq!(router.mirror().call().await?._0, *mirror.address());
        assert_eq!(
            router.mirrorProxy().call().await?._0,
            *mirror_proxy.address()
        );

        Ok(Self {
            router_address,
            wvara_address,
            provider,
        })
    }

    pub fn provider(&self) -> Arc<AlloyProvider> {
        self.provider.clone()
    }

    pub fn router(&self) -> Router {
        Router::new(
            self.router_address,
            self.wvara_address,
            self.provider.clone(),
        )
    }

    pub fn mirror(&self, address: LocalAddress) -> Mirror {
        Mirror::new(address.0.into(), self.provider.clone())
    }
}

async fn create_provider(
    rpc_url: &str,
    signer: LocalSigner,
    sender_address: LocalAddress,
) -> Result<Arc<AlloyProvider>> {
    Ok(Arc::new(
        ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(EthereumWallet::new(Sender::new(signer, sender_address)?))
            .on_builtin(rpc_url)
            .await?,
    ))
}

#[derive(Debug, Clone)]
struct Sender {
    signer: LocalSigner,
    sender: PublicKey,
    chain_id: Option<ChainId>,
}

impl Sender {
    pub fn new(signer: LocalSigner, sender_address: LocalAddress) -> Result<Self> {
        let sender = signer
            .get_key_by_addr(sender_address)?
            .ok_or_else(|| anyhow!("no key found for {sender_address}"))?;

        Ok(Self {
            signer,
            sender,
            chain_id: None,
        })
    }
}

#[async_trait]
impl Signer for Sender {
    async fn sign_hash(&self, hash: &B256) -> SignerResult<Signature> {
        self.sign_hash_sync(hash)
    }

    fn address(&self) -> Address {
        self.sender.to_address().0.into()
    }

    fn chain_id(&self) -> Option<ChainId> {
        self.chain_id
    }

    fn set_chain_id(&mut self, chain_id: Option<ChainId>) {
        self.chain_id = chain_id;
    }
}

#[async_trait]
impl TxSigner<Signature> for Sender {
    fn address(&self) -> Address {
        self.sender.to_address().0.into()
    }

    async fn sign_transaction(
        &self,
        tx: &mut dyn SignableTransaction<Signature>,
    ) -> SignerResult<Signature> {
        sign_transaction_with_chain_id!(self, tx, self.sign_hash_sync(&tx.signature_hash()))
    }
}

impl SignerSync for Sender {
    fn sign_hash_sync(&self, hash: &B256) -> SignerResult<Signature> {
        let signature = self
            .signer
            .raw_sign_digest(self.sender, hash.0.into())
            .map_err(|err| SignerError::Other(err.into()))?;
        Ok(Signature::try_from(signature.as_ref())?)
    }

    fn chain_id_sync(&self) -> Option<ChainId> {
        self.chain_id
    }
}

// TODO: Maybe better to append solution like this to alloy.
trait TryGetReceipt<T: Transport + Clone, N: Network> {
    /// Works like `self.get_receipt().await`, but retries a few times if rpc returns a null response.
    async fn try_get_receipt(self) -> Result<N::ReceiptResponse>;
}

impl<T: Transport + Clone, N: Network> TryGetReceipt<T, N> for PendingTransactionBuilder<'_, T, N> {
    async fn try_get_receipt(self) -> Result<N::ReceiptResponse> {
        let tx_hash = *self.tx_hash();
        let provider = self.provider().clone();

        let mut err = match self.get_receipt().await {
            Ok(r) => return Ok(r),
            Err(err) => err,
        };

        for _ in 0..3 {
            match err {
                PendingTransactionError::TransportError(RpcError::NullResp) => {}
                _ => break,
            }

            tokio::time::sleep(Duration::from_millis(100)).await;

            match provider.get_transaction_receipt(tx_hash).await {
                Ok(Some(r)) => return Ok(r),
                Ok(None) => {}
                Err(e) => err = e.into(),
            }
        }

        Err(anyhow!(
            "Failed to get transaction receipt for {tx_hash}: {err}"
        ))
    }
}

pub(crate) fn decode_log<E: SolEvent>(log: &Log) -> Result<E> {
    E::decode_raw_log(log.topics(), &log.data().data, false).map_err(Into::into)
}

macro_rules! signatures_consts {
    (
        $type_name:ident;
        $( $const_name:ident: $name:ident, )*
    ) => {
        $(
            pub const $const_name: alloy::primitives::B256 = $type_name::$name::SIGNATURE_HASH;
        )*

        pub const ALL: &[alloy::primitives::B256] = &[$($const_name,)*];
    };
}

pub(crate) use signatures_consts;
