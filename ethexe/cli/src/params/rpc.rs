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
use clap::Parser;
use ethexe_processor::{DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER, RunnerConfig};
use ethexe_rpc::RpcConfig;
use ethexe_service::config::NodeConfig;
use serde::Deserialize;
use std::{
    net::{Ipv4Addr, SocketAddr},
    str::FromStr,
};

/// Parameters for the RPC service to start.
#[derive(Clone, Debug, Deserialize, Parser)]
#[serde(deny_unknown_fields)]
pub struct RpcParams {
    /// Port to expose RPC service.
    #[arg(long)]
    #[serde(rename = "port")]
    pub rpc_port: Option<u16>,

    /// Flag to expose RPC service on all interfaces.
    #[arg(long)]
    #[serde(default, rename = "external")]
    pub rpc_external: bool,

    /// CORS policy for RPC service.
    #[arg(long)]
    #[serde(rename = "cors")]
    pub rpc_cors: Option<Cors>,

    /// Flag to disable RPC service.
    #[arg(long)]
    #[serde(default, rename = "no-rpc")]
    pub no_rpc: bool,

    #[arg(long)]
    pub gas_limit_multiplier: Option<u64>,
}

impl RpcParams {
    /// Default RPC port.
    pub const DEFAULT_RPC_PORT: u16 = 9944;

    /// Convert self into a proper `RpcConfig` object, if RPC service is enabled.
    pub fn into_config(self, node_config: &NodeConfig) -> Option<RpcConfig> {
        if self.no_rpc {
            return None;
        }

        let ipv4_addr = if self.rpc_external {
            Ipv4Addr::UNSPECIFIED
        } else {
            Ipv4Addr::LOCALHOST
        };

        let listen_addr = SocketAddr::new(
            ipv4_addr.into(),
            self.rpc_port.unwrap_or(Self::DEFAULT_RPC_PORT),
        );

        let cors = self
            .rpc_cors
            .unwrap_or_else(|| {
                Cors::List(vec![
                    "http://localhost:*".into(),
                    "http://127.0.0.1:*".into(),
                    "https://localhost:*".into(),
                    "https://127.0.0.1:*".into(),
                ])
            })
            .into();

        let gas_limit_multiplier = self
            .gas_limit_multiplier
            .unwrap_or(DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER);

        let runner_config = RunnerConfig::overlay(
            node_config.chunk_processing_threads,
            node_config.block_gas_limit,
            gas_limit_multiplier,
        );

        Some(RpcConfig {
            listen_addr,
            cors,
            runner_config,
        })
    }
}

impl MergeParams for RpcParams {
    fn merge(self, with: Self) -> Self {
        Self {
            rpc_port: self.rpc_port.or(with.rpc_port),
            rpc_external: self.rpc_external || with.rpc_external,
            rpc_cors: self.rpc_cors.or(with.rpc_cors),
            no_rpc: self.no_rpc || with.no_rpc,
            gas_limit_multiplier: self.gas_limit_multiplier.or(with.gas_limit_multiplier),
        }
    }
}

/// Enum for ease of parsing and deserializing CORS policy.
#[derive(Clone, Debug)]
pub enum Cors {
    /// Allow all origins.
    All,
    /// Allow only specified origins.
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

impl<'de> Deserialize<'de> for Cors {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: toml::Value = Deserialize::deserialize(deserializer)?;

        match value {
            toml::Value::String(s) if matches!(s.as_ref(), "all" | "*") => Ok(Self::All),
            toml::Value::Array(arr) => arr
                .into_iter()
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| serde::de::Error::custom("Array items must be strings"))
                        .map(|s| s.to_string())
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Self::List),
            _ => Err(serde::de::Error::custom(
                "Invalid value for cors. Possible values: \"all\" (alias \"*\") or list of strings like [\"http://localhost:*\", \"https://127.0.0.1:*\"].",
            )),
        }
    }
}
