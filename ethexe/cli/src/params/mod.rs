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

use anyhow::{Context, Result};
use clap::Parser;
use ethexe_service::config::Config;
use serde::Deserialize;
use std::path::PathBuf;

mod ethereum;
mod network;
mod node;
mod prometheus;
mod rpc;

pub use ethereum::EthereumParams;
pub use network::NetworkParams;
pub use node::NodeParams;
pub use prometheus::PrometheusParams;
pub use rpc::RpcParams;

/// CLI/TOML-config parameters for the ethexe service.
#[derive(Clone, Debug, Default, Deserialize, Parser)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// General various node and execution parameters.
    #[clap(flatten)]
    pub node: Option<NodeParams>,

    /// Ethereum-specific parameters.
    #[clap(flatten)]
    #[serde(alias = "eth")]
    pub ethereum: Option<EthereumParams>,

    /// Network service related parameters.
    #[clap(flatten)]
    #[serde(alias = "net")]
    pub network: Option<NetworkParams>,

    /// Ethexe RPC service hosting parameters.
    #[clap(flatten)]
    pub rpc: Option<RpcParams>,

    /// Prometheus (metrics) service parameters.
    #[clap(flatten)]
    pub prometheus: Option<PrometheusParams>,
}

impl Params {
    /// Load the parameters from a TOML file.
    pub fn from_file(path: PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path).context("failed to read params file")?;
        let params = toml::from_str(&content).context("failed to parse toml params file")?;

        Ok(params)
    }

    /// Convert self into a proper services `Config` object.
    pub fn into_config(self) -> Result<Config> {
        let Params {
            node,
            ethereum,
            network,
            rpc,
            prometheus,
        } = self;

        let node = node.context("missing node params")?;
        let net_dir = node.net_dir();

        let ethereum = ethereum.context("missing ethereum params")?;
        let node = node.into_config()?;
        let ethereum = ethereum.into_config()?;
        let network = network
            .and_then(|p| p.into_config(net_dir, ethereum.router_address).transpose())
            .transpose()?;
        let rpc = rpc.and_then(|p| p.into_config(&node));
        let prometheus = prometheus.and_then(|p| p.into_config());
        Ok(Config {
            node,
            ethereum,
            network,
            rpc,
            prometheus,
        })
    }
}

impl MergeParams for Params {
    fn merge(self, with: Self) -> Self {
        Self {
            node: MergeParams::optional_merge(self.node, with.node),
            ethereum: MergeParams::optional_merge(self.ethereum, with.ethereum),
            network: MergeParams::optional_merge(self.network, with.network),
            rpc: MergeParams::optional_merge(self.rpc, with.rpc),
            prometheus: MergeParams::optional_merge(self.prometheus, with.prometheus),
        }
    }
}

/// Helper trait for merging parameters of two sources: from cli and file.
pub trait MergeParams: Sized {
    /// Merge two parameter, self must be prioritized.
    fn merge(self, with: Self) -> Self;

    /// Optionally merge two parameters.
    fn optional_merge(me: Option<Self>, with: Option<Self>) -> Option<Self> {
        match (me, with) {
            (Some(me), Some(with)) => Some(me.merge(with)),
            (Some(me), None) => Some(me),
            (None, Some(with)) => Some(with),
            (None, None) => None,
        }
    }
}
