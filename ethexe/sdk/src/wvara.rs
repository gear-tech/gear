// This file is part of Gear.

// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use alloy::rpc::types::TransactionReceipt;
use anyhow::Result;
use ethexe_ethereum::wvara::{
    WVara as EthereumWVara, WVaraEvents as EthereumWVaraEvents, WVaraQuery as EthereumWVaraQuery,
};
use gprimitives::{ActorId, H256, U256};

pub struct WVara {
    pub(crate) wvara_client: EthereumWVara,
    pub(crate) wvara_query_client: EthereumWVaraQuery,
}

impl WVara {
    pub fn events(&self) -> EthereumWVaraEvents<'_> {
        self.wvara_query_client.events()
    }

    pub async fn name(&self) -> Result<String> {
        self.wvara_query_client.name().await
    }

    pub async fn symbol(&self) -> Result<String> {
        self.wvara_query_client.symbol().await
    }

    pub async fn decimals(&self) -> Result<u8> {
        self.wvara_query_client.decimals().await
    }

    pub async fn total_supply(&self) -> Result<u128> {
        self.wvara_query_client.total_supply().await
    }

    pub async fn balance_of(&self, address: ActorId) -> Result<u128> {
        self.wvara_query_client.balance_of(address).await
    }

    pub async fn transfer(&self, to: ActorId, value: u128) -> Result<H256> {
        self.wvara_client.transfer(to, value).await
    }

    pub async fn transfer_with_receipt(
        &self,
        to: ActorId,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self.wvara_client.transfer_with_receipt(to, value).await
    }

    pub async fn transfer_from(&self, from: ActorId, to: ActorId, value: u128) -> Result<H256> {
        self.wvara_client.transfer_from(from, to, value).await
    }

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

    pub async fn approve(&self, spender: ActorId, value: u128) -> Result<H256> {
        self.wvara_client.approve(spender, value).await
    }

    pub async fn approve_with_receipt(
        &self,
        spender: ActorId,
        value: u128,
    ) -> Result<TransactionReceipt> {
        self.wvara_client.approve_with_receipt(spender, value).await
    }

    pub async fn approve_all(&self, spender: ActorId) -> Result<H256> {
        self.wvara_client.approve_all(spender).await
    }

    pub async fn approve_all_with_receipt(&self, spender: ActorId) -> Result<TransactionReceipt> {
        self.wvara_client.approve_all_with_receipt(spender).await
    }

    pub async fn allowance(&self, owner: ActorId, spender: ActorId) -> Result<U256> {
        self.wvara_query_client.allowance(owner, spender).await
    }

    pub async fn mint(&self, to: ActorId, amount: u128) -> Result<H256> {
        self.wvara_client.mint(to, amount).await
    }

    pub async fn mint_with_receipt(&self, to: ActorId, amount: u128) -> Result<TransactionReceipt> {
        self.wvara_client.mint_with_receipt(to, amount).await
    }
}
