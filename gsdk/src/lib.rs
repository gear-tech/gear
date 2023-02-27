// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Gear api
use client::RpcClient;
use config::GearConfig;
use core::ops::{Deref, DerefMut};
pub use result::{Error, Result};
pub use signer::PairSigner;
use signer::Signer;
use std::sync::Arc;
pub use subxt::dynamic::Value;
use subxt::OnlineClient;

mod client;
pub mod config;
mod constants;
pub mod events;
pub mod metadata;
pub mod result;
mod rpc;
pub mod signer;
mod storage;
pub mod types;
mod utils;
pub mod ext {
    pub use sp_core;
    pub use sp_runtime;
}

/// Gear api wrapper.
#[derive(Clone)]
pub struct Api {
    /// How many times we'll retry when rpc requests failed.
    pub retry: u16,
    client: OnlineClient<GearConfig>,
}

impl Api {
    /// Create new API client.
    pub async fn new(url: Option<&str>) -> Result<Self> {
        Self::new_with_timeout(url, None).await
    }

    /// Create new API client with timeout.
    pub async fn new_with_timeout(url: Option<&str>, timeout: Option<u64>) -> Result<Self> {
        Ok(Self {
            // Retry our failed RPC requests for 5 times by default.
            retry: 5,
            client: OnlineClient::from_rpc_client(Arc::new(RpcClient::new(url, timeout).await?))
                .await?,
        })
    }

    /// Setup retry times and return the API instance.
    pub fn with_retry(mut self, retry: u16) -> Self {
        self.retry = retry;
        self
    }

    /// Subscribe all blocks
    pub async fn blocks(&self) -> Result<types::Blocks> {
        Ok(types::Blocks(self.client.blocks().subscribe_all().await?))
    }

    /// Subscribe finalized blocks
    pub async fn finalized_blocks(&self) -> Result<types::Blocks> {
        Ok(types::Blocks(
            self.client.blocks().subscribe_finalized().await?,
        ))
    }

    /// New signer from api
    pub fn signer(self, suri: &str, passwd: Option<&str>) -> Result<Signer> {
        Signer::new(self, suri, passwd)
    }
}

impl Deref for Api {
    type Target = OnlineClient<GearConfig>;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl DerefMut for Api {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.client
    }
}
