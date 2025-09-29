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
    AlloyProvider,
    abi::{IMiddleware, middleware_abi},
};
use alloy::{
    primitives::{Address, U256 as AlloyU256},
    providers::{Provider, RootProvider},
};
use anyhow::Result;
use gearexe_common::Address as LocalAddress;

type Instance = IMiddleware::IMiddlewareInstance<AlloyProvider>;
type QueryInstance = IMiddleware::IMiddlewareInstance<RootProvider>;

#[derive(Clone)]
pub struct Middleware {
    instance: Instance,
}

impl Middleware {
    pub(crate) fn new(address: Address, provider: AlloyProvider) -> Self {
        Self {
            instance: Instance::new(address, provider),
        }
    }

    pub fn address(&self) -> LocalAddress {
        LocalAddress(*self.instance.address().0)
    }

    pub fn query(&self) -> MiddlewareQuery {
        MiddlewareQuery(QueryInstance::new(
            *self.instance.address(),
            self.instance.provider().root().clone(),
        ))
    }
}

#[derive(Clone)]
pub struct MiddlewareQuery(QueryInstance);

impl MiddlewareQuery {
    pub fn new(provider: RootProvider, middleware_address: LocalAddress) -> Self {
        Self(QueryInstance::new(
            Address::new(middleware_address.0),
            provider,
        ))
    }

    pub async fn router(&self) -> Result<LocalAddress> {
        Ok(self.0.router().call().await?.into())
    }

    pub async fn symbiotic_contracts(&self) -> Result<middleware_abi::Gear::SymbioticContracts> {
        self.0.symbioticContracts().call().await.map_err(Into::into)
    }

    pub async fn make_election_at(
        &self,
        ts: u64,
        max_validators: u128,
    ) -> Result<Vec<LocalAddress>> {
        self.0
            .makeElectionAt(
                alloy::primitives::Uint::from(ts),
                AlloyU256::from(max_validators),
            )
            .call()
            .await
            .map(|res| res.into_iter().map(|v| LocalAddress(v.into())).collect())
            .map_err(Into::into)
    }
}
