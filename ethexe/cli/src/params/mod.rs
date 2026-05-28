// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Shared configuration model for the `ethexe` CLI.
//!
//! The same structures are deserialized from TOML and parsed from clap, which keeps the
//! config file shape aligned with the command-line interface. Command handlers merge their
//! explicit CLI values over the file-loaded values through [`MergeParams`].

use anyhow::{Context, Result};
use clap::Parser;
use ethexe_service::config::Config;
use serde::Deserialize;
use std::path::PathBuf;

mod ethereum;
mod malachite;
mod network;
mod node;
mod prometheus;
mod rpc;

pub use ethereum::EthereumParams;
pub use malachite::MalachiteParams;
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

    /// Malachite consensus service parameters.
    #[clap(flatten)]
    #[serde(alias = "mala")]
    pub malachite: Option<MalachiteParams>,

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

    /// Converts merged CLI/TOML parameters into a runtime [`Config`].
    ///
    /// `node` and `ethereum` are required because every service configuration depends on them.
    /// The remaining sections are optional and are omitted when the corresponding service is
    /// disabled or not configured.
    pub fn into_config(self) -> Result<Config> {
        let Params {
            node,
            ethereum,
            network,
            malachite,
            rpc,
            prometheus,
        } = self;

        let node = node.context("missing node params")?;
        let net_dir = node.net_dir();
        let is_dev = node.dev;

        let ethereum = ethereum.context("missing ethereum params")?;
        let node = node.into_config()?;
        let ethereum = ethereum.into_config()?;
        let network = network
            .and_then(|p| {
                p.into_config(net_dir, ethereum.router_address, is_dev)
                    .transpose()
            })
            .transpose()?;
        let malachite = malachite.unwrap_or_default().into_config()?;
        let rpc = rpc.and_then(|p| p.into_config(&node));
        let prometheus = prometheus.and_then(|p| p.into_config());
        Ok(Config {
            node,
            ethereum,
            network,
            malachite,
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
            malachite: MergeParams::optional_merge(self.malachite, with.malachite),
            rpc: MergeParams::optional_merge(self.rpc, with.rpc),
            prometheus: MergeParams::optional_merge(self.prometheus, with.prometheus),
        }
    }
}

/// Helper trait for merging parameters of two sources: from cli and file.
pub trait MergeParams: Sized {
    /// Merges two parameter values, keeping `self` as the higher-priority source.
    fn merge(self, with: Self) -> Self;

    /// Merges optional parameter sections while preserving the same priority order.
    fn optional_merge(me: Option<Self>, with: Option<Self>) -> Option<Self> {
        match (me, with) {
            (Some(me), Some(with)) => Some(me.merge(with)),
            (Some(me), None) => Some(me),
            (None, Some(with)) => Some(with),
            (None, None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `deny_unknown_fields` is on every section, so any stale or
    // misnamed knob in the shipped examples would fail at runtime
    // the moment an operator uncomments it. Parse the bundled
    // template directly to catch drift between code and example.
    #[test]
    fn example_toml_parses() {
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../.ethexe.example.toml"
        ));
        toml::from_str::<Params>(content).expect(".ethexe.example.toml must stay parseable");
    }

    #[test]
    fn example_local_toml_parses() {
        let content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../.ethexe.example.local.toml"
        ));
        toml::from_str::<Params>(content).expect(".ethexe.example.local.toml must stay parseable");
    }
}
