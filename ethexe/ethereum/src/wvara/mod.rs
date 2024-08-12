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

use crate::{abi::IWrappedVara, AlloyProvider, AlloyTransport};
use alloy::{
    primitives::{Address, Uint},
    providers::{ProviderBuilder, RootProvider},
    transports::BoxTransport,
};
use anyhow::Result;
use ethexe_signer::Address as LocalAddress;
use gprimitives::H256;
use std::sync::Arc;

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

    // TODO (breathx): handle events.
    pub async fn approve(&self, address: Address, value: u128) -> Result<H256> {
        let value = Uint::<256, 4>::from(value);
        let builder = self.0.approve(address, value);
        let tx = builder.send().await?;

        let receipt = tx.get_receipt().await?;

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
}
