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

use crate::{
    AlloyProvider, TryGetReceipt,
    abi::{self, IWrappedVara, utils},
};
use alloy::{
    primitives::{Address as AlloyAddress, U256 as AlloyU256},
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::TransactionReceipt,
};
use anyhow::Result;
use events::{AllEventsBuilder, ApprovalEventBuilder, TransferEventBuilder};
use gprimitives::{ActorId, H256, U256};
use gsigner::Address;

pub mod events;

type Instance = IWrappedVara::IWrappedVaraInstance<AlloyProvider>;
type QueryInstance = IWrappedVara::IWrappedVaraInstance<RootProvider>;

pub struct WVara(Instance);

impl WVara {
    pub(crate) fn new(address: AlloyAddress, provider: AlloyProvider) -> Self {
        Self(Instance::new(address, provider))
    }

    pub fn address(&self) -> Address {
        (*self.0.address()).into()
    }

    pub fn query(&self) -> WVaraQuery {
        WVaraQuery(QueryInstance::new(
            *self.0.address(),
            self.0.provider().root().clone(),
        ))
    }

    pub async fn transfer(&self, to: ActorId, value: u128) -> Result<H256> {
        self.transfer_with_receipt(to, value)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

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

    pub async fn transfer_from(&self, from: ActorId, to: ActorId, value: u128) -> Result<H256> {
        self.transfer_from_with_receipt(from, to, value)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

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

    pub async fn approve(&self, spender: ActorId, value: u128) -> Result<H256> {
        self.approve_with_receipt(spender, value)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

    pub async fn approve_with_receipt(
        &self,
        spender: ActorId,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self._approve_with_receipt(spender, U256::from(value)).await
    }

    pub async fn approve_all(&self, spender: ActorId) -> Result<H256> {
        self.approve_all_with_receipt(spender)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

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

    pub async fn mint(&self, to: ActorId, amount: u128) -> Result<H256> {
        self.mint_with_receipt(to, amount)
            .await
            .map(|receipt| (*receipt.transaction_hash).into())
    }

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

pub struct WVaraQuery(QueryInstance);

impl WVaraQuery {
    pub async fn new(rpc_url: &str, router_address: Address) -> Result<Self> {
        let provider = ProviderBuilder::default().connect(rpc_url).await?;

        Ok(Self(QueryInstance::new(
            AlloyAddress::new(router_address.0),
            provider,
        )))
    }

    pub fn events(&self) -> WVaraEvents<'_> {
        WVaraEvents { query: self }
    }

    pub async fn name(&self) -> Result<String> {
        self.0
            .name()
            .call()
            .await
            .map(|res| res.to_string())
            .map_err(Into::into)
    }

    pub async fn symbol(&self) -> Result<String> {
        self.0
            .symbol()
            .call()
            .await
            .map(|res| res.to_string())
            .map_err(Into::into)
    }

    pub async fn decimals(&self) -> Result<u8> {
        self.0.decimals().call().await.map_err(Into::into)
    }

    pub async fn total_supply(&self) -> Result<u128> {
        self.0
            .totalSupply()
            .call()
            .await
            .map(abi::utils::uint256_to_u128_lossy)
            .map_err(Into::into)
    }

    pub async fn balance_of(&self, address: ActorId) -> Result<u128> {
        self.0
            .balanceOf(address.into())
            .call()
            .await
            .map(abi::utils::uint256_to_u128_lossy)
            .map_err(Into::into)
    }

    pub async fn allowance(&self, owner: ActorId, spender: ActorId) -> Result<U256> {
        self.0
            .allowance(owner.into(), spender.into())
            .call()
            .await
            .map(|res| U256(res.into_limbs()))
            .map_err(Into::into)
    }
}

pub struct WVaraEvents<'a> {
    query: &'a WVaraQuery,
}

impl<'a> WVaraEvents<'a> {
    pub fn all(&self) -> AllEventsBuilder<'a> {
        AllEventsBuilder::new(self.query)
    }

    pub fn transfer(&self) -> TransferEventBuilder<'a> {
        TransferEventBuilder::new(self.query)
    }

    pub fn approval(&self) -> ApprovalEventBuilder<'a> {
        ApprovalEventBuilder::new(self.query)
    }
}
