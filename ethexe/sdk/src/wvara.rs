// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use alloy::rpc::types::TransactionReceipt;
use anyhow::Result;
use ethexe_ethereum::wvara::{
    WVara as EthereumWVara, WVaraEvents as EthereumWVaraEvents, WVaraQuery as EthereumWVaraQuery,
};
use gprimitives::{ActorId, H256, U256};

/// SDK handle for the WrappedVara ERC-20 contract, bundling a signing client and a read-only query client.
///
/// Obtain via [`VaraEthApi::wrapped_vara`]. Methods that mutate state (transfer, approve, mint)
/// use the signing client; query methods (name, balance_of, allowance, …) use the read-only client.
pub struct WVara {
    pub(crate) wvara_client: EthereumWVara,
    pub(crate) wvara_query_client: EthereumWVaraQuery,
}

impl WVara {
    /// Returns an event-builder facade for subscribing to WrappedVara contract events.
    pub fn events(&self) -> EthereumWVaraEvents<'_> {
        self.wvara_query_client.events()
    }

    /// Returns the ERC-20 token name stored on-chain.
    pub async fn name(&self) -> Result<String> {
        self.wvara_query_client.name().await
    }

    /// Returns the ERC-20 token symbol stored on-chain.
    pub async fn symbol(&self) -> Result<String> {
        self.wvara_query_client.symbol().await
    }

    /// Returns the number of decimal places used by the token (typically 18).
    pub async fn decimals(&self) -> Result<u8> {
        self.wvara_query_client.decimals().await
    }

    /// Returns the total token supply currently in circulation.
    pub async fn total_supply(&self) -> Result<u128> {
        self.wvara_query_client.total_supply().await
    }

    /// Returns the token balance held by `address`.
    pub async fn balance_of(&self, address: ActorId) -> Result<u128> {
        self.wvara_query_client.balance_of(address).await
    }

    /// Transfers `value` tokens to `to`, returning the transaction hash on success.
    pub async fn transfer(&self, to: ActorId, value: u128) -> Result<H256> {
        self.wvara_client.transfer(to, value).await
    }

    /// Transfers `value` tokens to `to`, returning the full transaction receipt on success.
    pub async fn transfer_with_receipt(
        &self,
        to: ActorId,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self.wvara_client.transfer_with_receipt(to, value).await
    }

    /// Transfers `value` tokens from `from` to `to` using the caller's allowance, returning the transaction hash.
    pub async fn transfer_from(&self, from: ActorId, to: ActorId, value: u128) -> Result<H256> {
        self.wvara_client.transfer_from(from, to, value).await
    }

    /// Transfers `value` tokens from `from` to `to` using the caller's allowance, returning the full transaction receipt.
    pub async fn transfer_from_with_receipt(
        &self,
        from: ActorId,
        to: ActorId,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self.wvara_client
            .transfer_from_with_receipt(from, to, value)
            .await
    }

    /// Approves `spender` to spend up to `value` tokens on behalf of the caller, returning the transaction hash.
    pub async fn approve(&self, spender: ActorId, value: u128) -> Result<H256> {
        self.wvara_client.approve(spender, value).await
    }

    /// Approves `spender` to spend up to `value` tokens, returning the full transaction receipt.
    pub async fn approve_with_receipt(
        &self,
        spender: ActorId,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self.wvara_client.approve_with_receipt(spender, value).await
    }

    /// Grants `spender` an unlimited allowance (`U256::MAX`), returning the transaction hash.
    pub async fn approve_all(&self, spender: ActorId) -> Result<H256> {
        self.wvara_client.approve_all(spender).await
    }

    /// Grants `spender` an unlimited allowance (`U256::MAX`), returning the full transaction receipt.
    pub async fn approve_all_with_receipt(&self, spender: ActorId) -> Result<TransactionReceipt> {
        self.wvara_client.approve_all_with_receipt(spender).await
    }

    /// Returns the remaining number of tokens that `spender` is allowed to spend on behalf of `owner`.
    pub async fn allowance(&self, owner: ActorId, spender: ActorId) -> Result<U256> {
        self.wvara_query_client.allowance(owner, spender).await
    }

    /// Returns the EIP-2612 permit nonce for `owner`, used to prevent replay attacks on signed approvals.
    pub async fn nonces(&self, owner: ActorId) -> Result<U256> {
        self.wvara_query_client.nonces(owner).await
    }

    /// Mints `amount` new tokens to `to`, returning the transaction hash on success.
    pub async fn mint(&self, to: ActorId, amount: u128) -> Result<H256> {
        self.wvara_client.mint(to, amount).await
    }

    /// Mints `amount` new tokens to `to`, returning the full transaction receipt on success.
    pub async fn mint_with_receipt(&self, to: ActorId, amount: u128) -> Result<TransactionReceipt> {
        self.wvara_client.mint_with_receipt(to, amount).await
    }
}
