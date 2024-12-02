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

use clap::Args;
use ethexe_rpc::RpcConfig;
use serde::Deserialize;
use std::{
    net::{Ipv4Addr, SocketAddr},
    str::FromStr,
};

/// Parameters used to config prometheus.
#[derive(Debug, Clone, Args, Deserialize)]
pub struct RpcParams {
    /// Rpc endpoint port.
    #[arg(long, default_value = "9944")]
    pub rpc_port: u16,

    /// Expose rpc endpoint on all interfaces
    #[arg(long, default_value = "false")]
    pub rpc_external: bool,

    /// Do not start rpc endpoint.
    #[arg(long, default_value = "false")]
    pub no_rpc: bool,

    /// Specify browser *origins* allowed to access the HTTP & WS RPC servers.
    ///
    /// A comma-separated list of origins (protocol://domain or special `null`
    /// value). Value of `all` will disable origin validation. Default is to
    /// allow localhost origin.
    #[arg(long)]
    pub rpc_cors: Option<Cors>,
}

impl RpcParams {
    /// Creates [`RpcConfig`].
    pub fn as_config(&self) -> Option<RpcConfig> {
        if self.no_rpc {
            return None;
        };

        let ip = if self.rpc_external {
            Ipv4Addr::UNSPECIFIED
        } else {
            Ipv4Addr::LOCALHOST
        }
        .into();

        let listen_addr = SocketAddr::new(ip, self.rpc_port);

        let cors = self
            .rpc_cors
            .clone()
            .unwrap_or_else(|| {
                Cors::List(vec![
                    "http://localhost:*".into(),
                    "http://127.0.0.1:*".into(),
                    "https://localhost:*".into(),
                    "https://127.0.0.1:*".into(),
                ])
            })
            .into();

        Some(RpcConfig { listen_addr, cors })
    }
}

/// CORS setting
///
/// The type is introduced to overcome `Option<Option<T>>` handling of `clap`.
#[derive(Clone, Debug, Deserialize)]
pub enum Cors {
    /// All hosts allowed.
    All,
    /// Only hosts on the list are allowed.
    List(Vec<String>),
}

impl From<Cors> for Option<Vec<String>> {
    fn from(cors: Cors) -> Self {
        match cors {
            Cors::All => None,
            Cors::List(list) => Some(list),
        }
    }
}

impl FromStr for Cors {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut is_all = false;
        let mut origins = Vec::new();
        for part in s.split(',') {
            match part {
                "all" | "*" => {
                    is_all = true;
                    break;
                }
                other => origins.push(other.to_owned()),
            }
        }

        if is_all {
            Ok(Cors::All)
        } else {
            Ok(Cors::List(origins))
        }
    }
}
