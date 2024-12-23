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

use super::MergeParams;
use anyhow::{Context, Result};
use clap::Parser;
use ethexe_network::{
    export::{Multiaddr, Protocol},
    NetworkEventLoopConfig as NetworkConfig,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Parser)]
#[serde(deny_unknown_fields)]
pub struct NetworkParams {
    #[arg(long, alias = "net-key")]
    #[serde(rename = "key")]
    pub network_key: Option<String>,

    #[arg(long, aliases = &["net-bootnodes", "bootnodes"])]
    #[serde(rename = "bootnodes")]
    pub network_bootnodes: Option<Vec<Multiaddr>>,

    #[arg(long, aliases = &["net-public-addr", "public-addr"])]
    #[serde(rename = "public-addr")]
    pub network_public_addr: Option<Vec<Multiaddr>>,

    #[arg(long, aliases = &["net-listen-addr", "listen-addr"])]
    #[serde(rename = "listen-addr")]
    pub network_listen_addr: Option<Vec<Multiaddr>>,

    #[arg(long, alias = "net-port")]
    #[serde(rename = "port")]
    pub network_port: Option<u16>,

    #[arg(long, alias = "no-net")]
    #[serde(default, rename = "no-network", alias = "no-net")]
    pub no_network: bool,
}

impl NetworkParams {
    pub const DEFAULT_NETWORK_PORT: u16 = 20333;

    pub fn into_config(self, config_dir: PathBuf) -> Result<Option<NetworkConfig>> {
        if self.no_network {
            return Ok(None);
        }

        let public_key = self
            .network_key
            .map(|k| k.parse())
            .transpose()
            .with_context(|| "invalid `network-key`")?;

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
            config_dir,
            public_key,
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
