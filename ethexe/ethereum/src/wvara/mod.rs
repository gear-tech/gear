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
    abi::{self, IWrappedVara},
    AlloyProvider, AlloyTransport, TryGetReceipt,
};
use alloy::{
    primitives::{Address, U256 as AlloyU256},
    providers::{Provider, ProviderBuilder, RootProvider},
    transports::BoxTransport,
};
use anyhow::Result;
use ethexe_signer::Address as LocalAddress;
use gprimitives::{H256, U256};
use std::sync::Arc;

pub mod events;

type InstanceProvider = Arc<AlloyProvider>;
type Instance = IWrappedVara::IWrappedVaraInstance<AlloyTransport, InstanceProvider>;

type QueryInstance =
    IWrappedVara::IWrappedVaraInstance<AlloyTransport, Arc<RootProvider<BoxTransport>>>;

pub struct WVara(Instance);

impl WVara {
    pub(crate) fn new(address: Address, provider: InstanceProvider) -> Self {
        Self(Instance::new(address, provider))
    }

    pub fn address(&self) -> LocalAddress {
        LocalAddress(*self.0.address().0)
    }

    pub fn query(&self) -> WVaraQuery {
        WVaraQuery(QueryInstance::new(
            *self.0.address(),
            Arc::new(self.0.provider().root().clone()),
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
        let provider = Arc::new(ProviderBuilder::new().on_builtin(rpc_url).await?);

        Ok(Self(QueryInstance::new(
            Address::new(router_address.0),
            provider,
        )))
    }

    pub async fn decimals(&self) -> Result<u8> {
        self.0
            .decimals()
            .call()
            .await
            .map(|res| res._0)
            .map_err(Into::into)
    }

    pub async fn total_supply(&self) -> Result<u128> {
        self.0
            .totalSupply()
            .call()
            .await
            .map(|res| abi::utils::uint256_to_u128_lossy(res._0))
            .map_err(Into::into)
    }

    pub async fn balance_of(&self, address: Address) -> Result<u128> {
        self.0
            .balanceOf(address)
            .call()
            .await
            .map(|res| abi::utils::uint256_to_u128_lossy(res._0))
            .map_err(Into::into)
    }

    pub async fn allowance(&self, owner: Address, spender: Address) -> Result<U256> {
        self.0
            .allowance(owner, spender)
            .call()
            .await
            .map(|res| U256(res._0.into_limbs()))
            .map_err(Into::into)
    }
}
