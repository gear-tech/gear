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

use crate::{
    abi::{
        IMirror, IRouter, IWrappedVara,
        utils::{self as abi_utils, Permit},
    },
    wvara::WVaraQuery,
};
use alloy::{
    consensus::SignableTransaction,
    eips::BlockId,
    network::{self, Ethereum as AlloyEthereum, EthereumWallet, Network, TxSigner},
    primitives::{Address as AlloyAddress, B256, ChainId, Signature, U256 as AlloyU256, address},
    providers::{
        Identity, PendingTransactionBuilder, PendingTransactionError, Provider, ProviderBuilder,
        RootProvider, WalletProvider,
        fillers::{
            BlobGasEstimator, BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill,
            NonceFiller, SimpleNonceManager, WalletFiller,
        },
        utils::{self, Eip1559Estimator},
    },
    rpc::types::{TransactionReceipt, TransactionRequest, eth::Log},
    signers::{
        self as alloy_signer, Error as SignerError, Result as SignerResult, Signer as AlloySigner,
        SignerSync, sign_transaction_with_chain_id,
    },
    sol_types::{SolEvent, SolStruct},
    transports::RpcError,
};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use ethexe_common::{BlockHeader, Digest, SimpleBlockData, ecdsa::PublicKey};
use gprimitives::{ActorId, H256, MessageId};
use gsigner::secp256k1::{Address, Secp256k1SignerExt, Signer};
use middleware::Middleware;
use mirror::Mirror;
use router::{Router, RouterQuery};
use std::time::Duration;

pub mod abi;
pub mod deploy;
pub mod middleware;
pub mod mirror;
pub mod router;
pub mod wvara;

pub mod primitives {
    pub use alloy::primitives::*;
}

pub type AlloyProvider = FillProvider<
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

#[derive(Debug, Clone, Default)]
pub struct EthereumBuilder {
    rpc_url: Option<String>,
    router_address: Option<Address>,
    signer: Option<Signer>,
    sender_address: Option<Address>,
    eip1559_fee_increase_percentage: Option<u64>,
    blob_gas_multiplier: Option<u128>,
    initialize_addresses: Option<bool>,
}

impl EthereumBuilder {
    /// Sets the Ethereum RPC URL to connect to.
    ///
    /// The default is [`Ethereum::DEFAULT_ETHEREUM_RPC`].
    pub fn rpc_url(mut self, rpc_url: impl Into<String>) -> Self {
        self.rpc_url = Some(rpc_url.into());
        self
    }

    /// Sets the Ethereum router contract address.
    ///
    /// The default is [`Ethereum::DEFAULT_ROUTER_ADDRESS`].
    pub fn router_address(mut self, router_address: Address) -> Self {
        self.router_address = Some(router_address);
        self
    }

    /// Sets the signer to use for signing transactions.
    pub fn signer(mut self, signer: Signer) -> Self {
        self.signer = Some(signer);
        self
    }

    /// Sets the sender address to use for signing transactions.
    pub fn sender_address(mut self, sender_address: Address) -> Self {
        self.sender_address = Some(sender_address);
        self
    }

    /// Sets the EIP-1559 fee increase percentage (from "medium") to use for transaction fee estimation.
    pub fn eip1559_fee_increase_percentage_opt(
        mut self,
        eip1559_fee_increase_percentage: Option<u64>,
    ) -> Self {
        self.eip1559_fee_increase_percentage = eip1559_fee_increase_percentage;
        self
    }

    /// Sets the EIP-1559 fee increase percentage (from "medium") to use for transaction fee estimation.
    ///
    /// The default is [`Ethereum::NO_EIP1559_FEE_INCREASE_PERCENTAGE`].
    pub fn eip1559_fee_increase_percentage(self, eip1559_fee_increase_percentage: u64) -> Self {
        self.eip1559_fee_increase_percentage_opt(Some(eip1559_fee_increase_percentage))
    }

    /// Sets the EIP-1559 fee increase percentage to value that increases the estimated fee
    /// by 15% compared to "medium" estimation.
    pub fn with_eip1559_increased_fee(self) -> Self {
        self.eip1559_fee_increase_percentage(Ethereum::INCREASED_EIP1559_FEE_INCREASE_PERCENTAGE)
    }

    /// Sets the blob gas multiplier to use for transaction fee estimation.
    pub fn blob_gas_multiplier_opt(mut self, blob_gas_multiplier: Option<u128>) -> Self {
        self.blob_gas_multiplier = blob_gas_multiplier;
        self
    }

    /// Sets the blob gas multiplier to use for transaction fee estimation.
    ///
    /// The default is [`Ethereum::NO_BLOB_GAS_MULTIPLIER`].
    pub fn blob_gas_multiplier(self, blob_gas_multiplier: u128) -> Self {
        self.blob_gas_multiplier_opt(Some(blob_gas_multiplier))
    }

    /// Sets the blob gas multiplier to value that increases the estimated blob gas by 3x.
    pub fn with_increased_blob_gas_multiplier(self) -> Self {
        self.blob_gas_multiplier(Ethereum::INCREASED_BLOB_GAS_MULTIPLIER)
    }

    /// Sets whether to initialize the router-related addresses (wvara and middleware)
    /// during the construction of the [`Ethereum`] instance.
    pub(crate) fn initialize_addresses(mut self, initialize_addresses: bool) -> Self {
        self.initialize_addresses = Some(initialize_addresses);
        self
    }

    /// Sets whether to initialize the router-related addresses (wvara and middleware)
    /// during the construction of the [`Ethereum`] instance.
    pub(crate) fn without_initializing_addresses(self) -> Self {
        self.initialize_addresses(false)
    }

    /// Constructs [`Ethereum`] instance from the builder.
    pub async fn build(self) -> Result<Ethereum> {
        let rpc_url = self
            .rpc_url
            .unwrap_or_else(|| Ethereum::DEFAULT_ETHEREUM_RPC.into());
        let router_address = self
            .router_address
            .unwrap_or(Ethereum::DEFAULT_ROUTER_ADDRESS.into());
        let signer = self.signer.context("signer is required")?;
        let sender_address = self.sender_address.context("sender address is required")?;
        let eip1559_fee_increase_percentage = self
            .eip1559_fee_increase_percentage
            .unwrap_or(Ethereum::NO_EIP1559_FEE_INCREASE_PERCENTAGE);
        let blob_gas_multiplier = self
            .blob_gas_multiplier
            .unwrap_or(Ethereum::NO_BLOB_GAS_MULTIPLIER);
        let initialize_addresses = self.initialize_addresses.unwrap_or(true);

        Ethereum::new(
            &rpc_url,
            router_address,
            signer,
            sender_address,
            eip1559_fee_increase_percentage,
            blob_gas_multiplier,
            initialize_addresses,
        )
        .await
    }
}

#[derive(Debug)]
pub(crate) struct Eip712PermitData {
    pub deadline: AlloyU256,
    pub v: u8,
    pub r: B256,
    pub s: B256,
}

#[derive(Clone)]
pub struct Ethereum {
    router: AlloyAddress,
    wvara: AlloyAddress,
    /// NOTE: Middleware address will be zero if `with_middleware` flag was not passed
    /// for [`deploy::EthereumDeployer`].
    middleware: AlloyAddress,
    provider: AlloyProvider,
    signer: Signer,
    sender_address: Address,
}

impl Ethereum {
    /// Default Ethereum RPC.
    pub const DEFAULT_ETHEREUM_RPC: &str = "ws://localhost:8545";
    /// Default Ethereum router contract address.
    pub const DEFAULT_ROUTER_ADDRESS: AlloyAddress =
        address!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9");

    /// Default EIP-1559 fee increase percentage for transaction fee estimation.
    pub const NO_EIP1559_FEE_INCREASE_PERCENTAGE: u64 = 0;
    /// EIP-1559 fee increase percentage for transaction fee estimation that increases the estimated fee
    /// by 15% compared to "medium" estimation.
    ///
    /// This is useful for faster batch commitment.
    pub const INCREASED_EIP1559_FEE_INCREASE_PERCENTAGE: u64 = 15;

    /// Default blob gas multiplier for transaction fee estimation.
    pub const NO_BLOB_GAS_MULTIPLIER: u128 = 1;
    /// Blob gas multiplier for transaction fee estimation that increases the estimated blob gas by 3x.
    ///
    /// This is useful mostly on testnets, where a lot of L2s can spam the network with blob transactions.
    pub const INCREASED_BLOB_GAS_MULTIPLIER: u128 = 3;

    /// Default offset for permit deadline from the current block timestamp.
    pub const PERMIT_DEADLINE_OFFSET: u64 = 120; // 2 minutes

    pub(crate) async fn new(
        ethereum_rpc_url: &str,
        router_address: Address,
        signer: Signer,
        sender_address: Address,
        eip1559_fee_increase_percentage: u64,
        blob_gas_multiplier: u128,
        initialize_addresses: bool,
    ) -> Result<Ethereum> {
        let provider = create_provider(
            ethereum_rpc_url,
            signer.clone(),
            sender_address,
            eip1559_fee_increase_percentage,
            blob_gas_multiplier,
        )
        .await?;
        let router_query = RouterQuery::from_provider(router_address, provider.root().clone());
        let router = router_address.into();
        let (wvara, middleware) = if initialize_addresses {
            (
                router_query.wvara_address().await?.into(),
                router_query.middleware_address().await?.into(),
            )
        } else {
            (AlloyAddress::ZERO, AlloyAddress::ZERO)
        };
        Ok(Self {
            router,
            wvara,
            middleware,
            provider,
            signer,
            sender_address,
        })
    }

    pub(crate) async fn initialize_addresses(&mut self) -> Result<()> {
        if self.wvara == AlloyAddress::ZERO && self.middleware == AlloyAddress::ZERO {
            let router_query =
                RouterQuery::from_provider(self.router, self.provider.root().clone());
            self.wvara = router_query.wvara_address().await?.into();
            self.middleware = router_query.middleware_address().await?.into();
        }
        Ok(())
    }

    pub fn signer(&self) -> &Signer {
        &self.signer
    }

    pub(crate) fn sender(&self) -> Sender {
        Sender::new(self.signer.clone(), self.sender_address).expect("infallible")
    }

    pub fn sender_address(&self) -> Address {
        self.sender_address
    }

    pub fn provider(&self) -> AlloyProvider {
        self.provider.clone()
    }

    pub async fn chain_id(&self) -> Result<u64> {
        self.provider.get_chain_id().await.map_err(Into::into)
    }

    pub(crate) async fn get_latest_block_inner(
        provider: &AlloyProvider,
    ) -> Result<SimpleBlockData> {
        Self::get_block_inner(provider, BlockId::latest()).await
    }

    pub(crate) async fn get_block_inner(
        provider: &AlloyProvider,
        block_id: impl IntoBlockId,
    ) -> Result<SimpleBlockData> {
        let block_resp = provider
            .get_block(block_id.into_block_id())
            .await
            .with_context(|| "failed to get latest block")?
            .ok_or_else(|| anyhow!("latest block not found"))?;
        let height = block_resp
            .number()
            .try_into()
            .with_context(|| "block number overflow")?;
        let hash = block_resp.hash().0.into();
        let header = block_resp.into_header();
        let parent_hash = header.parent_hash.0.into();
        let timestamp = header.timestamp;
        let header = BlockHeader {
            height,
            timestamp,
            parent_hash,
        };
        Ok(SimpleBlockData { hash, header })
    }

    pub async fn get_latest_block(&self) -> Result<SimpleBlockData> {
        Self::get_latest_block_inner(&self.provider()).await
    }

    pub async fn get_block(&self, block_id: impl IntoBlockId) -> Result<SimpleBlockData> {
        Self::get_block_inner(&self.provider(), block_id).await
    }

    pub(crate) async fn prepare_permit_data(
        provider: &AlloyProvider,
        wvara_query: WVaraQuery,
        sender: &Sender,
        spender: ActorId,
        value: u128,
    ) -> Result<Eip712PermitData> {
        let signer_address = provider.default_signer_address();
        let signer_actor_id = Address::from(signer_address).into();

        let nonce = abi_utils::u256_to_uint256(wvara_query.nonces(signer_actor_id).await?);
        let eip712_domain = wvara_query.eip712_domain().await?;
        let SimpleBlockData {
            header: BlockHeader { timestamp, .. },
            ..
        } = Self::get_latest_block_inner(provider).await?;
        let deadline = AlloyU256::from(
            timestamp
                .checked_add(Self::PERMIT_DEADLINE_OFFSET)
                .expect("infallible"),
        );

        let permit = Permit {
            owner: signer_address,
            spender: spender.into(),
            value: AlloyU256::from(value),
            nonce,
            deadline,
        };

        let hash = permit.eip712_signing_hash(&eip712_domain);
        let signature = sender.sign_hash(&hash).await?;

        let v = (signature.v() as u8) + 27;
        let r = signature.r().into();
        let s = signature.s().into();

        Ok(Eip712PermitData { deadline, v, r, s })
    }

    pub fn mirror(&self, actor_id: ActorId) -> Mirror {
        Mirror::new(actor_id.into(), self.wvara, self.sender(), self.provider())
    }

    pub fn router(&self) -> Router {
        Router::new(self.router, self.wvara, self.sender(), self.provider())
    }

    pub fn wrapped_vara(&self) -> WVara {
        WVara::new(self.wvara, self.provider())
    }

    pub fn middleware(&self) -> Middleware {
        assert_ne!(
            self.middleware,
            AlloyAddress::ZERO,
            "Middleware address is zero. Make sure to deploy the middleware contract and pass `with_middleware` flag to `EthereumDeployer`."
        );
        Middleware::new(self.middleware, self.provider())
    }
}

pub(crate) async fn create_provider(
    rpc_url: &str,
    signer: Signer,
    sender_address: Address,
    eip1559_fee_increase_percentage: u64,
    blob_gas_multiplier: u128,
) -> Result<AlloyProvider> {
    Ok(ProviderBuilder::default()
        .with_eip1559_estimator(Eip1559Estimator::new(move |base_fee_per_gas, rewards| {
            utils::eip1559_default_estimator(base_fee_per_gas, rewards)
                .scaled_by_pct(eip1559_fee_increase_percentage)
        }))
        .with_blob_gas_estimator(BlobGasEstimator::scaled(blob_gas_multiplier))
        .with_simple_nonce_management()
        .fetch_chain_id()
        .wallet(EthereumWallet::new(Sender::new(signer, sender_address)?))
        .connect(rpc_url)
        .await?)
}

#[derive(Debug, Clone)]
struct Sender {
    signer: Signer,
    sender: PublicKey,
    chain_id: Option<ChainId>,
}

impl Sender {
    pub fn new(signer: Signer, sender_address: Address) -> Result<Self> {
        let sender = signer
            .get_key_by_address(sender_address)?
            .ok_or_else(|| anyhow!("no key found for {sender_address}"))?;

        Ok(Self {
            signer,
            sender,
            chain_id: None,
        })
    }
}

#[async_trait]
impl AlloySigner for Sender {
    async fn sign_hash(&self, hash: &B256) -> SignerResult<Signature> {
        self.sign_hash_sync(hash)
    }

    fn address(&self) -> AlloyAddress {
        self.sender.to_address().into()
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
    fn address(&self) -> AlloyAddress {
        self.sender.to_address().into()
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
        let digest = Digest(hash.0);
        let signature = self
            .signer
            .sign_digest(self.sender, digest, None)
            .map_err(|err| SignerError::Other(err.into()))?;
        Signature::from_raw(&signature.as_raw_bytes()).map_err(|err| SignerError::Other(err.into()))
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

impl IntoBlockId for u64 {
    fn into_block_id(self) -> BlockId {
        BlockId::number(self)
    }
}

impl IntoBlockId for BlockId {
    fn into_block_id(self) -> BlockId {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sender_signs_prehashed_message() {
        let signer = Signer::memory();
        let public_key = signer.generate().unwrap();
        let address = signer.address(public_key);

        let sender = Sender::new(signer.clone(), address).expect("sender init");

        let hash = B256::from([0xAA; 32]);
        let signature = sender.sign_hash_sync(&hash).expect("signature");

        let recovered_vk = signature.recover_from_prehash(&hash).expect("recover");
        let recovered_bytes: [u8; 33] = recovered_vk
            .to_encoded_point(true)
            .as_bytes()
            .try_into()
            .expect("compressed size");
        let recovered_address = gsigner::secp256k1::PublicKey::from_bytes(recovered_bytes)
            .expect("valid public key")
            .to_address();

        assert_eq!(recovered_address, address);
    }
}
