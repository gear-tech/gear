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

//! Application config in one place.

use anyhow::Result;
use ethexe_network::NetworkConfig;
use ethexe_observer::EthereumConfig;
use ethexe_prometheus::PrometheusConfig;
use ethexe_rpc::RpcConfig;
use ethexe_signer::PublicKey;
use std::{path::PathBuf, str::FromStr};

#[derive(Debug)]
pub struct Config {
    pub node: NodeConfig,
    pub ethereum: EthereumConfig,
    pub network: Option<NetworkConfig>,
    pub rpc: Option<RpcConfig>,
    pub prometheus: Option<PrometheusConfig>,
}

impl Config {
    pub fn log_info(&self) {
        log::info!("💾 Database: {}", self.node.database_path.display());
        log::info!("🔑 Key directory: {}", self.node.key_path.display());
        if let Some(net_config) = &self.network {
            log::info!("🛜  Network directory: {}", net_config.config_dir.display());
        }
        log::info!("⧫  Ethereum observer RPC: {}", self.ethereum.rpc);
        log::info!(
            "📡 Ethereum router address: {}",
            self.ethereum.router_address
        );
    }
}

#[derive(Debug)]
pub struct NodeConfig {
    pub database_path: PathBuf,
    pub key_path: PathBuf,
    pub sequencer: ConfigPublicKey,
    pub validator: ConfigPublicKey,
    pub max_commitment_depth: u32,
    pub worker_threads_override: Option<usize>,
    pub virtual_threads: usize,
    pub dev: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConfigPublicKey {
    Enabled(PublicKey),
    Random,
    #[default]
    Disabled,
}

impl ConfigPublicKey {
    pub fn new(key: &Option<String>) -> Result<Self> {
        match key {
            Some(key) => Self::from_str(key),
            None => Ok(Self::Disabled),
        }
    }
}

impl FromStr for ConfigPublicKey {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "random" => Ok(Self::Random),
            key => Ok(Self::Enabled(key.parse()?)),
        }
    }
}
