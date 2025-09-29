// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use abi::{IMirror, IRouter, IWrappedVara};
use alloy::{
    consensus::SignableTransaction,
    network::{Ethereum as AlloyEthereum, EthereumWallet, Network, TxSigner},
    primitives::{Address, B256, ChainId, Signature, SignatureError},
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
    sol_types::SolEvent,
    transports::RpcError,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use gearexe_common::{Address as LocalAddress, Digest, ecdsa::PublicKey};
use gearexe_signer::Signer as LocalSigner;
use middleware::Middleware;
use mirror::Mirror;
use router::{Router, RouterQuery};
use std::time::Duration;

mod abi;
mod eip1167;

pub mod deploy;
pub mod middleware;
pub mod mirror;
pub mod router;
pub mod wvara;

pub mod primitives {
    pub use alloy::primitives::*;
}

type AlloyRecommendedFillers = JoinFill<
    GasFiller,
    JoinFill<BlobGasFiller, JoinFill<NonceFiller<SimpleNonceManager>, ChainIdFiller>>,
>;
type AlloyProvider = FillProvider<ExeFiller, RootProvider, AlloyEthereum>;

pub(crate) type ExeFiller =
    JoinFill<JoinFill<Identity, AlloyRecommendedFillers>, WalletFiller<EthereumWallet>>;

pub struct Ethereum {
    router: Address,
    wvara: Address,
    /// NOTE: Middleware address will be zero if `with_middleware` flag was not passed
    /// for [`deploy::EthereumDeployer`].
    middleware: Address,
    provider: AlloyProvider,
}

impl Ethereum {
    pub async fn new(
        rpc: &str,
        router_address: Address,
        signer: LocalSigner,
        sender_address: LocalAddress,
    ) -> Result<Ethereum> {
        let provider = create_provider(rpc, signer, sender_address).await?;
        let router_query = RouterQuery::from_provider(router_address, provider.root().clone());
        Ok(Self {
            router: router_address,
            wvara: router_query.wvara_address().await?,
            middleware: router_query.middleware_address().await?,
            provider,
        })
    }

    pub async fn from_provider(provider: AlloyProvider, router: Address) -> Result<Self> {
        let router_query = RouterQuery::from_provider(router, provider.root().clone());
        Ok(Self {
            router,
            wvara: router_query.wvara_address().await?,
            middleware: router_query.middleware_address().await?,
            provider,
        })
    }
}

impl Ethereum {
    fn provider(&self) -> AlloyProvider {
        self.provider.clone()
    }

    pub fn mirror(&self, address: LocalAddress) -> Mirror {
        Mirror::new(address.0.into(), self.provider())
    }

    pub fn router(&self) -> Router {
        Router::new(self.router, self.wvara, self.provider())
    }

    pub fn middleware(&self) -> Option<Middleware> {
        if self.middleware == Address::ZERO {
            None
        } else {
            Some(Middleware::new(self.middleware, self.provider()))
        }
    }
}

pub(crate) async fn create_provider(
    rpc_url: &str,
    signer: LocalSigner,
    sender_address: LocalAddress,
) -> Result<AlloyProvider> {
    Ok(ProviderBuilder::default()
        .filler(AlloyRecommendedFillers::default())
        .wallet(EthereumWallet::new(Sender::new(signer, sender_address)?))
        .connect(rpc_url)
        .await?)
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
            .storage()
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
        let (s, r) = self
            .signer
            .sign(self.sender, Digest(hash.0))
            .map_err(|err| SignerError::Other(err.into()))
            .map(|s| s.into_parts())?;
        let v = r.to_byte() as u64;
        let v = primitives::normalize_v(v).ok_or(SignatureError::InvalidParity(v))?;
        Ok(Signature::from_signature_and_parity(s, v))
    }

    fn chain_id_sync(&self) -> Option<ChainId> {
        self.chain_id
    }
}

// TODO: Maybe better to append solution like this to alloy.
trait TryGetReceipt<N: Network> {
    /// Works like `self.get_receipt().await`, but retries a few times if rpc returns a null response.
    async fn try_get_receipt(self) -> Result<N::ReceiptResponse>;
}

impl<N: Network> TryGetReceipt<N> for PendingTransactionBuilder<N> {
    async fn try_get_receipt(self) -> Result<N::ReceiptResponse> {
        let tx_hash = *self.tx_hash();
        let provider = self.provider().clone();

        let mut err = match self.get_receipt().await {
            Ok(r) => return Ok(r),
            Err(err) => err,
        };

        log::trace!("Failed to get transaction receipt for {tx_hash}. Retrying...");
        for n in 0..20 {
            log::trace!("Attempt {n}. Error - {err}");
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
    E::decode_raw_log(log.topics(), &log.data().data).map_err(Into::into)
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
