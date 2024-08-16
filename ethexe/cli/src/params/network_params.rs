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

use anyhow::Context;
use clap::Args;
use ethexe_network::{
    utils::{Multiaddr, Protocol},
    NetworkEventLoopConfig,
};
use serde::Deserialize;
use std::path::PathBuf;

/// Parameters used to create the network configuration.
#[derive(Debug, Clone, Args, Deserialize)]
pub struct NetworkParams {
    /// Forced signer generated network key to be used inside `libp2p`
    #[arg(long, value_name = "NETWORK_KEY")]
    pub network_key: Option<String>,

    /// Specify a list of bootnodes.
    #[arg(long, value_name = "ADDR", num_args = 1..)]
    pub bootnodes: Vec<Multiaddr>,

    /// Public address that other nodes will use to connect to this node.
    ///
    /// This can be used if there's a proxy in front of this node.
    #[arg(long, value_name = "PUBLIC_ADDR", num_args = 1..)]
    pub public_addr: Vec<Multiaddr>,

    /// Listen on this multiaddress.
    ///
    /// By default:
    /// `/ip4/0.0.0.0/udp/<port>/quic-v1` and `/ip6/[::]/udp/<port>/quic-v1`.
    #[arg(long, value_name = "LISTEN_ADDR", num_args = 1..)]
    pub listen_addr: Vec<Multiaddr>,

    /// Specify p2p protocol TCP port.
    #[arg(long, value_name = "PORT", conflicts_with_all = &[ "listen_addr" ])]
    pub port: Option<u16>,

    /// Disable mDNS discovery (default: true).
    ///
    /// By default, the network will use mDNS to discover other nodes on the
    /// local network. This disables it. Automatically implied when using --dev.
    #[arg(long)]
    #[serde(default)]
    pub no_mdns: bool,
    // TODO: Add node key cli
    // #[allow(missing_docs)]
    // #[clap(flatten)]
    // pub node_key_params: NodeKeyParams,
}

impl NetworkParams {
    /// Fill the given `NetworkConfiguration` by looking at the cli parameters.
    pub fn network_config(self, net_path: PathBuf) -> anyhow::Result<NetworkEventLoopConfig> {
        let port = self.port.unwrap_or(ethexe_network::DEFAULT_LISTEN_PORT);

        let listen_addresses = if self.listen_addr.is_empty() {
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
            self.listen_addr.into_iter().collect()
        };

        let public_key = self
            .network_key
            .map(|k| k.parse())
            .transpose()
            .with_context(|| "Invalid network key")?;
        let external_addresses = self.public_addr.into_iter().collect();
        let bootstrap_addresses = self.bootnodes.into_iter().collect();

        Ok(NetworkEventLoopConfig {
            config_dir: net_path,
            public_key,
            external_addresses,
            bootstrap_addresses,
            listen_addresses,
            transport_type: Default::default(),
        })
    }
}
