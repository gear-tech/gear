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

use super::MergeParams;
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use ethexe_service::config::EthereumConfig;
use serde::Deserialize;
use std::time::Duration;

/// CLI/TOML-config parameters related to Ethereum.
#[derive(Clone, Debug, Deserialize, Parser)]
#[serde(deny_unknown_fields)]
pub struct EthereumParams {
    /// Ethereum RPC endpoint.
    #[arg(long, alias = "eth-rpc")]
    #[serde(rename = "rpc")]
    pub ethereum_rpc: Option<String>,

    /// Ethereum Beacon RPC endpoint.
    #[arg(long, alias = "eth-beacon-rpc")]
    #[serde(rename = "beacon-rpc")]
    pub ethereum_beacon_rpc: Option<String>,

    /// Ethereum router contract address.
    #[arg(long, alias = "eth-router")]
    #[serde(rename = "router")]
    pub ethereum_router: Option<String>,
}

impl EthereumParams {
    /// Default block time in seconds.
    pub const BLOCK_TIME: usize = 12;

    /// Default Ethereum RPC.
    pub const DEFAULT_ETHEREUM_RPC: &str = "http://localhost:8545";

    /// Default Ethereum Beacon RPC.
    pub const DEFAULT_ETHEREUM_BEACON_RPC: &str = "http://localhost:5052";

    /// Convert self into a proper `EthereumConfig` object.
    pub fn into_config(self) -> Result<EthereumConfig> {
        Ok(EthereumConfig {
            rpc: self
                .ethereum_rpc
                .unwrap_or_else(|| Self::DEFAULT_ETHEREUM_RPC.into()),
            beacon_rpc: self
                .ethereum_beacon_rpc
                .unwrap_or_else(|| Self::DEFAULT_ETHEREUM_BEACON_RPC.into()),
            router_address: self
                .ethereum_router
                .ok_or_else(|| anyhow!("missing `ethereum-router`"))?
                .parse()
                .with_context(|| "invalid `ethereum-router`")?,
            block_time: Duration::from_secs(Self::BLOCK_TIME as u64),
        })
    }
}

impl MergeParams for EthereumParams {
    fn merge(self, with: Self) -> Self {
        Self {
            ethereum_rpc: self.ethereum_rpc.or(with.ethereum_rpc),
            ethereum_beacon_rpc: self.ethereum_beacon_rpc.or(with.ethereum_beacon_rpc),
            ethereum_router: self.ethereum_router.or(with.ethereum_router),
        }
    }
}
