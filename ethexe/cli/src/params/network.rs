// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Parameters controlling the optional libp2p networking service.

use super::MergeParams;
use anyhow::{Context, Result};
use clap::Parser;
use ethexe_common::Address;
use ethexe_network::{
    NetworkConfig,
    export::{Multiaddr, Protocol},
};
use gsigner::secp256k1::Signer;
use serde::Deserialize;
use std::path::PathBuf;

/// Parameters for the networking service to start.
#[derive(Clone, Debug, Deserialize, Parser)]
#[serde(deny_unknown_fields)]
pub struct NetworkParams {
    /// Network pubkey of the node. If not provided, tries to fetch one from .net directory, in case of absence - generates and stores new random one.
    #[arg(long, alias = "net-key")]
    #[serde(rename = "key")]
    pub network_key: Option<String>,

    /// Predefined bootnodes addresses to connect to.
    #[arg(long, aliases = &["net-bootnodes", "bootnodes"])]
    #[serde(rename = "bootnodes")]
    pub network_bootnodes: Option<Vec<Multiaddr>>,

    /// Externally exposed network addresses of the node.
    #[arg(long, aliases = &["net-public-addr", "public-addr"])]
    #[serde(rename = "public-addr")]
    pub network_public_addr: Option<Vec<Multiaddr>>,

    /// Addresses to listen for incoming connections.
    #[arg(long, aliases = &["net-listen-addr", "listen-addr"])]
    #[serde(rename = "listen-addr")]
    pub network_listen_addr: Option<Vec<Multiaddr>>,

    /// Default network port.
    #[arg(long, alias = "net-port")]
    #[serde(rename = "port")]
    pub network_port: Option<u16>,

    /// Flag to disable network service.
    #[arg(long, alias = "no-net")]
    #[serde(default, rename = "no-network", alias = "no-net")]
    pub no_network: bool,
}

impl NetworkParams {
    /// Converts networking parameters into an optional [`NetworkConfig`].
    ///
    /// When networking is enabled, the method either parses an explicit network key or
    /// resolves one from the `net/` key store, generating it on first use.
    pub fn into_config(
        self,
        config_dir: PathBuf,
        router_address: Address,
        is_dev: bool,
    ) -> Result<Option<NetworkConfig>> {
        if self.no_network {
            return Ok(None);
        }

        let public_key = if let Some(key) = self.network_key {
            log::trace!("use network key from command-line arguments");
            key.parse().context("invalid network key")?
        } else {
            let signer = Signer::fs(config_dir)?;
            let keys = signer.list_keys()?;
            match keys.as_slice() {
                [] => {
                    log::trace!("generate a new network key");
                    signer.generate()?
                }
                [key] => {
                    log::trace!("use network key saved on disk");
                    *key
                }
                _ => anyhow::bail!("only one network key is expected"),
            }
        };

        let external_addresses = self
            .network_public_addr
            .unwrap_or_default()
            .into_iter()
            .collect();

        let bootstrap_addresses = self
            .network_bootnodes
            .unwrap_or_default()
            .into_iter()
            .collect();

        let network_listen_addr = self.network_listen_addr.unwrap_or_default();

        let port = self
            .network_port
            .unwrap_or(ethexe_network::DEFAULT_LISTEN_PORT);

        let listen_addresses = if network_listen_addr.is_empty() {
            [
                Multiaddr::empty()
                    .with(Protocol::Ip6([0, 0, 0, 0, 0, 0, 0, 0].into()))
                    .with(Protocol::Udp(port))
                    .with(Protocol::QuicV1),
                Multiaddr::empty()
                    .with(Protocol::Ip4([0, 0, 0, 0].into()))
                    .with(Protocol::Udp(port))
                    .with(Protocol::QuicV1),
            ]
            .into()
        } else {
            network_listen_addr.into_iter().collect()
        };

        Ok(Some(NetworkConfig {
            public_key,
            router_address,
            external_addresses,
            bootstrap_addresses,
            listen_addresses,
            transport_type: Default::default(),
            allow_non_global_addresses: is_dev,
        }))
    }
}

impl MergeParams for NetworkParams {
    fn merge(self, with: Self) -> Self {
        Self {
            network_key: self.network_key.or(with.network_key),
            network_bootnodes: self.network_bootnodes.or(with.network_bootnodes),
            network_public_addr: self.network_public_addr.or(with.network_public_addr),
            network_listen_addr: self.network_listen_addr.or(with.network_listen_addr),
            network_port: self.network_port.or(with.network_port),
            no_network: self.no_network || with.no_network,
        }
    }
}
