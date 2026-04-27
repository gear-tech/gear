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

//! Ethereum connectivity and fee-tuning parameters.

use super::MergeParams;
use anyhow::{Result, anyhow};
use clap::Parser;
use ethexe_common::Address;
use ethexe_ethereum::Ethereum;
use ethexe_service::config::EthereumConfig;
use serde::Deserialize;
use std::time::Duration;

/// CLI/TOML-config parameters related to Ethereum.
#[derive(Clone, Debug, Default, Deserialize, Parser)]
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
    pub ethereum_router: Option<Address>,

    /// Ethereum block time in seconds.
    #[arg(long, alias = "eth-block-time")]
    #[serde(rename = "block-time")]
    pub block_time: Option<u64>,

    /// EIP-1559 fee increase percentage (from "medium").
    #[arg(long, alias = "eip1559-fee-increase-percentage")]
    #[serde(rename = "eip1559-fee-increase-percentage")]
    pub eip1559_fee_increase_percentage: Option<u64>,

    /// Blob gas multiplier.
    #[arg(long, alias = "blob-gas-multiplier")]
    #[serde(rename = "blob-gas-multiplier")]
    pub blob_gas_multiplier: Option<u128>,
}

impl EthereumParams {
    /// Default block time in seconds.
    pub const BLOCK_TIME: u64 = 12;

    /// Default Ethereum RPC.
    pub const DEFAULT_ETHEREUM_RPC: &str = Ethereum::DEFAULT_ETHEREUM_RPC;

    /// Default Ethereum Beacon RPC.
    pub const DEFAULT_ETHEREUM_BEACON_RPC: &str = "http://localhost:8545";

    /// Default EIP-1559 fee increase percentage.
    pub const DEFAULT_EIP1559_FEE_INCREASE_PERCENTAGE: u64 =
        Ethereum::INCREASED_EIP1559_FEE_INCREASE_PERCENTAGE;

    /// Default blob gas multiplier.
    pub const DEFAULT_BLOB_GAS_MULTIPLIER: u128 = Ethereum::INCREASED_BLOB_GAS_MULTIPLIER;

    /// Converts Ethereum-facing CLI/TOML parameters into [`EthereumConfig`].
    ///
    /// The Router address is required because it anchors all on-chain operations. RPC
    /// endpoints, block time, and fee-tuning values fall back to sensible local defaults.
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
                .ok_or_else(|| anyhow!("missing `ethereum-router`"))?,
            block_time: Duration::from_secs(self.block_time.unwrap_or(Self::BLOCK_TIME)),
            eip1559_fee_increase_percentage: self
                .eip1559_fee_increase_percentage
                .unwrap_or(Self::DEFAULT_EIP1559_FEE_INCREASE_PERCENTAGE),
            blob_gas_multiplier: self
                .blob_gas_multiplier
                .unwrap_or(Self::DEFAULT_BLOB_GAS_MULTIPLIER),
        })
    }
}

impl MergeParams for EthereumParams {
    fn merge(self, with: Self) -> Self {
        Self {
            ethereum_rpc: self.ethereum_rpc.or(with.ethereum_rpc),
            ethereum_beacon_rpc: self.ethereum_beacon_rpc.or(with.ethereum_beacon_rpc),
            ethereum_router: self.ethereum_router.or(with.ethereum_router),
            block_time: self.block_time.or(with.block_time),
            eip1559_fee_increase_percentage: self
                .eip1559_fee_increase_percentage
                .or(with.eip1559_fee_increase_percentage),
            blob_gas_multiplier: self.blob_gas_multiplier.or(with.blob_gas_multiplier),
        }
    }
}
