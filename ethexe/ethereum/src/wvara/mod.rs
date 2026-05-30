// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
    AlloyProvider, TryGetReceipt,
    abi::{self, IWrappedVara, utils},
};
use alloy::{
    dyn_abi::Eip712Domain,
    primitives::{Address as AlloyAddress, U256 as AlloyU256},
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::TransactionReceipt,
    sol_types::eip712_domain,
};
use anyhow::Result;
use events::{AllEventsBuilder, ApprovalEventBuilder, TransferEventBuilder};
use gprimitives::{ActorId, H256, U256};
use gsigner::Address;

/// Event builders and event-extraction utilities for the WrappedVara ERC-20 contract.
pub mod events;

type Instance = IWrappedVara::IWrappedVaraInstance<AlloyProvider>;
type QueryInstance = IWrappedVara::IWrappedVaraInstance<RootProvider>;

/// Transaction-sending handle for the WrappedVara ERC-20 contract.
///
/// Wraps a signing provider and exposes mutating ERC-20 operations (transfer, approve, mint).
/// Use [`WVara::query`] to obtain a read-only [`WVaraQuery`] backed by the same contract address.
pub struct WVara(Instance);

impl WVara {
    pub(crate) fn new(address: AlloyAddress, provider: AlloyProvider) -> Self {
        Self(Instance::new(address, provider))
    }

    /// Returns the on-chain address of the WrappedVara contract.
    pub fn address(&self) -> Address {
        (*self.0.address()).into()
    }

    /// Creates a read-only [`WVaraQuery`] bound to the same contract address and root provider.
    pub fn query(&self) -> WVaraQuery {
        WVaraQuery(QueryInstance::new(
            *self.0.address(),
            self.0.provider().root().clone(),
        ))
    }

    /// Transfers `value` tokens to `to`, returning the transaction hash on success.
    pub async fn transfer(&self, to: ActorId, value: u128) -> Result<H256> {
        self.transfer_with_receipt(to, value)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Transfers `value` tokens to `to`, returning the full transaction receipt on success.
    pub async fn transfer_with_receipt(
        &self,
        to: ActorId,
        value: u128,
    ) -> Result<TransactionReceipt> {
        let builder = self.0.transfer(to.into(), AlloyU256::from(value));
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        Ok(receipt)
    }

    /// Transfers `value` tokens from `from` to `to` using the caller's allowance, returning the transaction hash.
    pub async fn transfer_from(&self, from: ActorId, to: ActorId, value: u128) -> Result<H256> {
        self.transfer_from_with_receipt(from, to, value)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Transfers `value` tokens from `from` to `to`, returning the full transaction receipt.
    pub async fn transfer_from_with_receipt(
        &self,
        from: ActorId,
        to: ActorId,
        value: u128,
    ) -> Result<TransactionReceipt> {
        let builder = self
            .0
            .transferFrom(from.into(), to.into(), AlloyU256::from(value));
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        Ok(receipt)
    }

    /// Approves `spender` to spend up to `value` tokens on behalf of the caller, returning the transaction hash.
    pub async fn approve(&self, spender: ActorId, value: u128) -> Result<H256> {
        self.approve_with_receipt(spender, value)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Approves `spender` to spend up to `value` tokens, returning the full transaction receipt.
    pub async fn approve_with_receipt(
        &self,
        spender: ActorId,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self._approve_with_receipt(spender, U256::from(value)).await
    }

    /// Approves `spender` to spend an unlimited amount (`U256::MAX`) of tokens, returning the transaction hash.
    pub async fn approve_all(&self, spender: ActorId) -> Result<H256> {
        self.approve_all_with_receipt(spender)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Approves `spender` for an unlimited allowance, returning the full transaction receipt.
    pub async fn approve_all_with_receipt(&self, spender: ActorId) -> Result<TransactionReceipt> {
        self._approve_with_receipt(spender, U256::MAX).await
    }

    async fn _approve_with_receipt(
        &self,
        spender: ActorId,
        value: U256,
    ) -> Result<TransactionReceipt> {
        let builder = self
            .0
            .approve(spender.into(), utils::u256_to_uint256(value));
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        Ok(receipt)
    }

    /// Mints `amount` new tokens to `to`, returning the transaction hash on success.
    pub async fn mint(&self, to: ActorId, amount: u128) -> Result<H256> {
        self.mint_with_receipt(to, amount)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    /// Mints `amount` new tokens to `to`, returning the full transaction receipt on success.
    pub async fn mint_with_receipt(&self, to: ActorId, amount: u128) -> Result<TransactionReceipt> {
        let builder = self.0.mint(to.into(), AlloyU256::from(amount));
        let receipt = builder
            .send()
            .await?
            .try_get_receipt_check_reverted()
            .await?;
        Ok(receipt)
    }
}

/// Read-only query handle for the WrappedVara ERC-20 contract.
///
/// Uses a root (unsigned) provider, so no transactions can be sent.
/// Obtain via [`WVara::query`] or construct directly with [`WVaraQuery::new`].
pub struct WVaraQuery(QueryInstance);

impl WVaraQuery {
    /// Constructs a `WVaraQuery` connected to `rpc_url` and bound to `router_address`.
    pub async fn new(rpc_url: &str, router_address: Address) -> Result<Self> {
        let provider = ProviderBuilder::default().connect(rpc_url).await?;

        Ok(Self(QueryInstance::new(
            AlloyAddress::new(router_address.0),
            provider,
        )))
    }

    /// Returns a [`WVaraEvents`] handle for subscribing to contract events.
    pub fn events(&self) -> WVaraEvents<'_> {
        WVaraEvents { query: self }
    }

    /// Returns the ERC-20 token name.
    pub async fn name(&self) -> Result<String> {
        self.0
            .name()
            .call()
            .await
            .map(|res| res.to_string())
            .map_err(Into::into)
    }

    /// Returns the ERC-20 token symbol.
    pub async fn symbol(&self) -> Result<String> {
        self.0
            .symbol()
            .call()
            .await
            .map(|res| res.to_string())
            .map_err(Into::into)
    }

    /// Returns the number of decimal places used by the token.
    pub async fn decimals(&self) -> Result<u8> {
        self.0.decimals().call().await.map_err(Into::into)
    }

    /// Returns the total token supply; truncates silently if the on-chain value exceeds `u128::MAX`.
    pub async fn total_supply(&self) -> Result<u128> {
        self.0
            .totalSupply()
            .call()
            .await
            .map(abi::utils::uint256_to_u128_lossy)
            .map_err(Into::into)
    }

    /// Returns the token balance of `address`; truncates silently if the value exceeds `u128::MAX`.
    pub async fn balance_of(&self, address: ActorId) -> Result<u128> {
        self.0
            .balanceOf(address.into())
            .call()
            .await
            .map(abi::utils::uint256_to_u128_lossy)
            .map_err(Into::into)
    }

    /// Returns the remaining number of tokens that `spender` is allowed to spend on behalf of `owner`.
    pub async fn allowance(&self, owner: ActorId, spender: ActorId) -> Result<U256> {
        self.0
            .allowance(owner.into(), spender.into())
            .call()
            .await
            .map(|res| U256(res.into_limbs()))
            .map_err(Into::into)
    }

    /// Returns the EIP-2612 permit nonce for `owner`, used to prevent replay attacks.
    pub async fn nonces(&self, owner: ActorId) -> Result<U256> {
        self.0
            .nonces(owner.into())
            .call()
            .await
            .map(|res| U256(res.into_limbs()))
            .map_err(Into::into)
    }

    pub(crate) async fn eip712_domain(&self) -> Result<Eip712Domain> {
        self.0
            .eip712Domain()
            .call()
            .await
            .map(|res| {
                eip712_domain! {
                    name: res.name,
                    version: res.version,
                    chain_id: res.chainId.try_into().expect("chainId should fit into u64"),
                    verifying_contract: res.verifyingContract,
                }
            })
            .map_err(Into::into)
    }
}

/// Entry point for subscribing to WrappedVara contract events.
///
/// Obtained from [`WVaraQuery::events`]. Each method returns a typed builder
/// that can be further filtered before calling `subscribe`.
pub struct WVaraEvents<'a> {
    query: &'a WVaraQuery,
}

impl<'a> WVaraEvents<'a> {
    /// Returns a builder that subscribes to all WrappedVara events (Transfer and Approval).
    pub fn all(&self) -> AllEventsBuilder<'a> {
        AllEventsBuilder::new(self.query)
    }

    /// Returns a builder for subscribing to ERC-20 Transfer events, optionally filtered by `from` or `to`.
    pub fn transfer(&self) -> TransferEventBuilder<'a> {
        TransferEventBuilder::new(self.query)
    }

    /// Returns a builder for subscribing to ERC-20 Approval events, optionally filtered by `owner` or `spender`.
    pub fn approval(&self) -> ApprovalEventBuilder<'a> {
        ApprovalEventBuilder::new(self.query)
    }
}
