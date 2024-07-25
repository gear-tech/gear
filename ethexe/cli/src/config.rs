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

//! Application config in one place.

use crate::args::Args;

use anyhow::{Context as _, Result};
use directories::ProjectDirs;
use ethexe_network::NetworkEventLoopConfig;
use ethexe_prometheus_endpoint::Registry;
use ethexe_signer::{Address, PublicKey};
use std::{iter, net::SocketAddr, path::PathBuf, str::FromStr, time::Duration};
use tempfile::TempDir;

const DEFAULT_PROMETHEUS_PORT: u16 = 9635;

#[static_init::dynamic(drop, lazy)]
static mut BASE_PATH_TEMP: Option<TempDir> = None;

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
pub enum ConfigPublicKey {
    Enabled(PublicKey),
    Random,
    #[default]
    Disabled,
}

impl ConfigPublicKey {
    fn new(key: &Option<String>) -> Result<Self> {
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

/// Configuration of the Prometheus endpoint.
#[derive(Debug, Clone)]
pub struct PrometheusConfig {
    /// Port to use.
    pub port: SocketAddr,
    /// A metrics registry to use. Useful for setting the metric prefix.
    pub registry: Registry,
}

impl PrometheusConfig {
    /// Create a new config using the default registry.
    pub fn new_with_default_registry(port: SocketAddr, chain_id: String) -> Self {
        let param = iter::once((String::from("chain"), chain_id)).collect();
        Self {
            port,
            registry: Registry::new_custom(None, Some(param))
                .expect("this can only fail if the prefix is empty"),
        }
    }
}

#[derive(Debug)]
pub struct Config {
    /// Name of node for telemetry
    pub node_name: String,

    /// RPC of the Ethereum endpoint
    pub ethereum_rpc: String,

    /// RPC of the Ethereum Beacon endpoint
    pub ethereum_beacon_rpc: String,

    /// Address of Ethereum Router contract
    pub ethereum_router_address: Address,

    /// Max depth to discover last commitment.
    pub max_commitment_depth: u32,

    /// Block production time.
    pub block_time: Duration,

    /// Path of the state database
    pub database_path: PathBuf,

    /// Signer key storage path
    pub key_path: PathBuf,

    /// Is this role a sequencer
    pub sequencer: ConfigPublicKey,

    /// Is this role a validator
    pub validator: ConfigPublicKey,

    /// Sender address to send Ethereum transaction.
    pub sender_address: Option<String>,

    // Network configuration
    pub net_config: NetworkEventLoopConfig,

    // Prometheus configuration
    pub prometheus_config: Option<PrometheusConfig>,

    /// RPC port
    pub rpc_port: u16,
}

impl TryFrom<Args> for Config {
    type Error = anyhow::Error;

    fn try_from(args: Args) -> Result<Self> {
        let base_path = if args.tmp {
            let mut temp = BASE_PATH_TEMP.write();

            match &*temp {
                Some(p) => p.path().into(),
                None => {
                    let temp_dir = tempfile::Builder::new().prefix("ethexe").tempdir()?;
                    let path = PathBuf::from(temp_dir.path());

                    *temp = Some(temp_dir);
                    path
                }
            }
        } else {
            match args.base_path {
                Some(r) => r,
                None => {
                    let proj_dirs = ProjectDirs::from("com", "Gear", "ethexe")
                        .with_context(|| "Invalid home directory path")?;
                    proj_dirs.data_dir().to_path_buf()
                }
            }
        };

        let chain_spec = match args.chain_spec.as_deref() {
            Some("testnet") => crate::chain_spec::testnet_config(),
            Some(path) => crate::chain_spec::from_file(path)?,
            _ => crate::chain_spec::testnet_config(),
        };

        let net_path = base_path.join("net");
        let mut net_config = args.network_params.network_config(net_path)?;
        net_config.bootstrap_addresses.extend(chain_spec.bootnodes);

        let sequencer =
            ConfigPublicKey::new(&args.sequencer_key).context("invalid sequencer key")?;
        anyhow::ensure!(
            args.tmp || sequencer != ConfigPublicKey::Random,
            "random key for sequencer is only allowed with `--tmp` flag"
        );

        let validator =
            ConfigPublicKey::new(&args.validator_key).context("invalid validator key")?;
        anyhow::ensure!(
            args.tmp || validator != ConfigPublicKey::Random,
            "random key for validator is only allowed with `--tmp` flag"
        );

        Ok(Config {
            node_name: args.node_name,
            ethereum_rpc: args.ethereum_rpc,
            ethereum_beacon_rpc: args.ethereum_beacon_rpc,
            ethereum_router_address: args
                .ethereum_router_address
                .unwrap_or(chain_spec.ethereum_router_address)
                .parse()
                .context("failed to parse router address")?,
            max_commitment_depth: args.max_commitment_depth.unwrap_or(1000),
            block_time: Duration::from_secs(args.block_time),
            net_config,
            prometheus_config: args.prometheus_params.and_then(|params| {
                params.prometheus_config(DEFAULT_PROMETHEUS_PORT, "ethexe-dev".to_string())
            }),
            database_path: base_path.join("db"),
            key_path: base_path.join("key"),
            sequencer,
            validator,
            sender_address: args.sender_address,
            rpc_port: args.rpc_port.unwrap_or(9090),
        })
    }
}
