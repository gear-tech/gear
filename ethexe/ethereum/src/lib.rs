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

use abi::{IMirror, IRouter};
use alloy::{
    consensus::SignableTransaction,
    eips::BlockId,
    network::{self, Ethereum as AlloyEthereum, EthereumWallet, Network, TxSigner},
    primitives::{Address, B256, ChainId, Signature, SignatureError},
    providers::{
        Identity, PendingTransactionBuilder, PendingTransactionError, Provider, ProviderBuilder,
        RootProvider,
        fillers::{
            BlobGasEstimator, BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill,
            NonceFiller, SimpleNonceManager, WalletFiller,
        },
    },
    rpc::types::{TransactionReceipt, TransactionRequest, eth::Log},
    signers::{
        self as alloy_signer, Error as SignerError, Result as SignerResult, Signer, SignerSync,
        sign_transaction_with_chain_id,
    },
    sol_types::SolEvent,
    transports::RpcError,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use ethexe_common::{Address as LocalAddress, Digest, ecdsa::PublicKey};
use ethexe_signer::Signer as LocalSigner;
use gprimitives::{H256, MessageId};
use middleware::Middleware;
use mirror::Mirror;
use router::{Router, RouterQuery};
use std::time::Duration;

mod eip1167;

pub mod abi;
pub mod deploy;
pub mod middleware;
pub mod mirror;
pub mod router;
pub mod wvara;

pub mod primitives {
    pub use alloy::primitives::*;
}

type AlloyProvider = FillProvider<
    JoinFill<
        JoinFill<
            JoinFill<
                JoinFill<JoinFill<Identity, GasFiller>, BlobGasFiller>,
                NonceFiller<SimpleNonceManager>,
            >,
            ChainIdFiller,
        >,
        WalletFiller<EthereumWallet>,
    >,
    RootProvider,
>;

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
    pub fn provider(&self) -> AlloyProvider {
        self.provider.clone()
    }

    pub fn mirror(&self, address: LocalAddress) -> Mirror {
        Mirror::new(address.0.into(), self.provider())
    }

    pub fn router(&self) -> Router {
        Router::new(self.router, self.wvara, self.provider())
    }

    pub fn wrapped_vara(&self) -> WVara {
        WVara::new(self.wvara, self.provider())
    }

    pub fn middleware(&self) -> Middleware {
        assert_ne!(
            self.middleware,
            Address::ZERO,
            "Middleware address is zero. Make sure to deploy the middleware contract and pass `with_middleware` flag to `EthereumDeployer`."
        );
        Middleware::new(self.middleware, self.provider())
    }
}

pub(crate) async fn create_provider(
    rpc_url: &str,
    signer: LocalSigner,
    sender_address: LocalAddress,
) -> Result<AlloyProvider> {
    Ok(ProviderBuilder::default()
        .with_gas_estimation()
        .with_blob_gas_estimator(BlobGasEstimator::scaled(3))
        .with_simple_nonce_management()
        .fetch_chain_id()
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

#[async_trait::async_trait]
pub trait TryGetReceipt<N: Network> {
    /// Works like `self.get_receipt().await`, but retries a few times if rpc returns a null response.
    async fn try_get_receipt(self) -> Result<N::ReceiptResponse>;

    /// Works like `self.try_get_receipt().await`, but also extracts the message id from the logs.
    async fn try_get_message_send_receipt(self) -> Result<(H256, MessageId)>;

    /// Works like `self.try_get_receipt().await`, but also checks if the transaction was reverted.
    async fn try_get_receipt_check_reverted(self) -> Result<N::ReceiptResponse>;
}

#[async_trait::async_trait]
impl TryGetReceipt<network::Ethereum> for PendingTransactionBuilder<network::Ethereum> {
    async fn try_get_receipt(self) -> Result<TransactionReceipt> {
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

    async fn try_get_message_send_receipt(self) -> Result<(H256, MessageId)> {
        let receipt = self.try_get_receipt().await?;
        let tx_hash = (*receipt.transaction_hash).into();
        let mut message_id = None;

        for log in receipt.inner.logs() {
            if log.topic0() == Some(&mirror::signatures::MESSAGE_QUEUEING_REQUESTED) {
                let event = crate::decode_log::<IMirror::MessageQueueingRequested>(log)?;

                message_id = Some((*event.id).into());

                break;
            }
        }

        let message_id =
            message_id.ok_or_else(|| anyhow!("Couldn't find `MessageQueueingRequested` log"))?;

        Ok((tx_hash, message_id))
    }

    async fn try_get_receipt_check_reverted(self) -> Result<TransactionReceipt> {
        let provider = self.provider().clone();
        let receipt = self.try_get_receipt().await?;

        let try_request_error_reason = async |provider: RootProvider| {
            let tx = provider
                .get_transaction_by_hash(receipt.transaction_hash)
                .await
                .ok()??;
            let request = TransactionRequest::from_recovered_transaction(tx.into_recovered());
            provider
                .call(request)
                .block(receipt.block_hash?.into())
                .await
                .err()
        };

        if receipt.status() {
            Ok(receipt)
        } else if let Some(err) = try_request_error_reason(provider).await {
            Err(anyhow!(
                "Transaction {:?} was reverted at block {:?}: {err}",
                receipt.transaction_hash,
                receipt.block_hash
            ))
        } else {
            Err(anyhow!(
                "Transaction {:?} was reverted by unknown reason at block {:?}",
                receipt.transaction_hash,
                receipt.block_hash
            ))
        }
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

use crate::wvara::WVara;

/// A helping trait for converting various types into `alloy::eips::BlockId`.
pub trait IntoBlockId {
    fn into_block_id(self) -> BlockId;
}

impl IntoBlockId for H256 {
    fn into_block_id(self) -> BlockId {
        BlockId::hash(self.0.into())
    }
}

impl IntoBlockId for u32 {
    fn into_block_id(self) -> BlockId {
        BlockId::number(self.into())
    }
}

impl IntoBlockId for BlockId {
    fn into_block_id(self) -> BlockId {
        self
    }
}
