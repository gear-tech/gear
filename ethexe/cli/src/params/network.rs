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
use anyhow::{Context, Result};
use clap::Parser;
use ethexe_common::Address;
use ethexe_network::{
    NetworkConfig,
    export::{Multiaddr, Protocol},
};
use ethexe_signer::Signer;
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
    /// Default network port.
    pub const DEFAULT_NETWORK_PORT: u16 = 20333;

    /// Convert self into a proper `NetworkConfig` object, if network is enabled.
    pub fn into_config(
        self,
        config_dir: PathBuf,
        router_address: Address,
    ) -> Result<Option<NetworkConfig>> {
        if self.no_network {
            return Ok(None);
        }

        let public_key = if let Some(key) = self.network_key {
            log::trace!("use network key from command-line arguments");
            key.parse().context("invalid network key")?
        } else {
            let signer = Signer::fs(config_dir);
            let keys = signer.storage_mut().list_keys()?;
            match keys.as_slice() {
                [] => {
                    log::trace!("generate a new network key");
                    signer.generate_key()?
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

        let port = self.network_port.unwrap_or(Self::DEFAULT_NETWORK_PORT);

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
