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
use gsigner::secp256k1::{Address, PublicKey};
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
        log::info!("⧫  Ethereum observer RPC: {}", self.ethereum.rpc);
        log::info!(
            "📡 Ethereum router address: {}",
            self.ethereum.router_address
        );
        if let Some(network) = &self.network {
            log::info!("🛜  Network public key: {}", network.public_key);
        }
    }

    /// Create a config clone for a secondary dev validator with its own database
    /// and network identity.
    pub fn clone_for_dev_validator(&self, key: &PublicKey, index: usize) -> Result<Config> {
        let tmp_dir = tempfile::Builder::new()
            .prefix(&format!("ethexe-validator-{index}"))
            .tempdir()
            .map_err(|e| anyhow::anyhow!("couldn't create temp dir for validator-{index}: {e}"))?;
        let db_path = tmp_dir.path().to_path_buf();
        // Leak the TempDir to keep it alive for the process lifetime
        std::mem::forget(tmp_dir);

        let network = self
            .network
            .as_ref()
            .map(|net| -> Result<_> {
                let net_keys_dir = self
                    .node
                    .key_path
                    .parent()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "key path '{}' has no parent directory",
                            self.node.key_path.display()
                        )
                    })?
                    .join("net");
                let signer = gsigner::secp256k1::Signer::fs(net_keys_dir).map_err(|e| {
                    anyhow::anyhow!("failed to open net keystore for dev validator: {e}")
                })?;
                let net_key = signer.generate().map_err(|e| {
                    anyhow::anyhow!("failed to generate network key for dev validator: {e}")
                })?;

                Ok(NetworkConfig {
                    public_key: net_key,
                    listen_addresses: ["/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap()].into(),
                    bootstrap_addresses: Default::default(),
                    external_addresses: Default::default(),
                    ..net.clone()
                })
            })
            .transpose()?;

        Ok(Config {
            node: NodeConfig {
                database_path: db_path,
                validator: ConfigPublicKey::Enabled(*key),
                validator_session: ConfigPublicKey::Enabled(*key),
                ..self.node.clone()
            },
            ethereum: self.ethereum.clone(),
            network,
            rpc: None,        // only primary node exposes RPC
            prometheus: None, // only primary node exposes metrics
        })
    }
}

#[derive(Clone, Debug)]
pub struct NodeConfig {
    pub database_path: PathBuf,
    pub key_path: PathBuf,
    pub validator: ConfigPublicKey,
    pub validator_session: ConfigPublicKey,
    pub eth_max_sync_depth: u32,
    pub worker_threads: Option<usize>,
    pub blocking_threads: Option<usize>,
    pub chunk_processing_threads: usize,
    pub block_gas_limit: u64,
    pub canonical_quarantine: u8,
    pub dev: bool,
    pub pre_funded_accounts: u32,
    pub fast_sync: bool,
    pub chain_deepness_threshold: u32,
}

impl NodeConfig {
    /// Return the path to the database for the given router address.
    pub fn database_path_for(&self, router_address: Address) -> PathBuf {
        let mut path = self.database_path.clone();
        path.push(router_address.to_string());
        path
    }
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
