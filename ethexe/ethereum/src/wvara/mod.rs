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
    abi::{self, IWrappedVara},
};
use alloy::{
    primitives::{Address, U256 as AlloyU256},
    providers::{Provider, ProviderBuilder, RootProvider},
};
use anyhow::Result;
use ethexe_common::Address as LocalAddress;
use gprimitives::{H256, U256};

pub mod events;

type Instance = IWrappedVara::IWrappedVaraInstance<AlloyProvider>;
type QueryInstance = IWrappedVara::IWrappedVaraInstance<RootProvider>;

pub struct WVara(Instance);

impl WVara {
    pub(crate) fn new(address: Address, provider: AlloyProvider) -> Self {
        Self(Instance::new(address, provider))
    }

    pub fn address(&self) -> LocalAddress {
        LocalAddress(*self.0.address().0)
    }

    pub fn query(&self) -> WVaraQuery {
        WVaraQuery(QueryInstance::new(
            *self.0.address(),
            self.0.provider().root().clone(),
        ))
    }

    pub async fn transfer(&self, to: Address, value: u128) -> Result<H256> {
        let builder = self.0.transfer(to, AlloyU256::from(value));
        let receipt = builder.send().await?.try_get_receipt().await?;

        let tx_hash = (*receipt.transaction_hash).into();

        Ok(tx_hash)
    }

    pub async fn transfer_from(&self, from: Address, to: Address, value: u128) -> Result<H256> {
        let builder = self.0.transferFrom(from, to, AlloyU256::from(value));
        let receipt = builder.send().await?.try_get_receipt().await?;

        let tx_hash = (*receipt.transaction_hash).into();

        Ok(tx_hash)
    }

    pub async fn approve(&self, address: Address, value: u128) -> Result<H256> {
        self._approve(address, AlloyU256::from(value)).await
    }

    pub async fn approve_all(&self, address: Address) -> Result<H256> {
        self._approve(address, AlloyU256::MAX).await
    }

    async fn _approve(&self, address: Address, value: AlloyU256) -> Result<H256> {
        let builder = self.0.approve(address, value);
        let receipt = builder.send().await?.try_get_receipt().await?;

        let tx_hash = (*receipt.transaction_hash).into();

        Ok(tx_hash)
    }
}

pub struct WVaraQuery(QueryInstance);

impl WVaraQuery {
    pub async fn new(rpc_url: &str, router_address: LocalAddress) -> Result<Self> {
        let provider = ProviderBuilder::default().connect(rpc_url).await?;

        Ok(Self(QueryInstance::new(
            Address::new(router_address.0),
            provider,
        )))
    }

    pub async fn from_provider(router_address: LocalAddress, provider: RootProvider) -> Self {
        Self(QueryInstance::new(Address::new(router_address.0), provider))
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

    pub async fn balance_of(&self, address: Address) -> Result<u128> {
        self.0
            .balanceOf(address)
            .call()
            .await
            .map(abi::utils::uint256_to_u128_lossy)
            .map_err(Into::into)
    }

    pub async fn allowance(&self, owner: Address, spender: Address) -> Result<U256> {
        self.0
            .allowance(owner, spender)
            .call()
            .await
            .map(|res| U256(res.into_limbs()))
            .map_err(Into::into)
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
}
