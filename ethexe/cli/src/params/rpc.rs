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
use anyhow::{Result, anyhow};
use clap::Parser;
use ethexe_rpc::{DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER, RpcConfig, SnapshotRpcConfig};
use ethexe_service::config::NodeConfig;
use serde::Deserialize;
use std::{
    net::{Ipv4Addr, SocketAddr},
    str::FromStr,
};

/// Parameters for the RPC service to start.
#[derive(Clone, Debug, Default, Deserialize, Parser)]
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

    /// Flag to enable snapshot download RPC API.
    #[arg(long)]
    #[serde(default)]
    pub snapshot: bool,

    /// Bearer token for snapshot download RPC authorization.
    #[arg(long)]
    #[serde(rename = "snapshot-token")]
    pub snapshot_token: Option<String>,

    /// Snapshot chunk size in bytes.
    #[arg(long)]
    #[serde(rename = "snapshot-chunk-bytes")]
    pub snapshot_chunk_bytes: Option<usize>,

    /// Snapshot retention period in seconds.
    #[arg(long)]
    #[serde(rename = "snapshot-retention-secs")]
    pub snapshot_retention_secs: Option<u64>,

    /// Max amount of concurrent snapshot downloads.
    #[arg(long)]
    #[serde(rename = "snapshot-max-concurrent")]
    pub snapshot_max_concurrent: Option<u32>,
}

impl RpcParams {
    /// Default RPC port.
    pub const DEFAULT_RPC_PORT: u16 = 9944;

    /// Convert self into a proper `RpcConfig` object, if RPC service is enabled.
    pub fn into_config(self, node_config: &NodeConfig) -> Result<Option<RpcConfig>> {
        if self.no_rpc {
            return Ok(None);
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

        let snapshot = if self.snapshot {
            let auth_bearer_token = self.snapshot_token.ok_or_else(|| {
                anyhow!("`snapshot-token` must be provided when `snapshot` rpc is enabled")
            })?;
            if auth_bearer_token.is_empty() {
                return Err(anyhow!(
                    "`snapshot-token` must be non-empty when `snapshot` rpc is enabled"
                ));
            }
            Some(SnapshotRpcConfig {
                auth_bearer_token,
                chunk_size_bytes: self
                    .snapshot_chunk_bytes
                    .unwrap_or(SnapshotRpcConfig::DEFAULT_CHUNK_SIZE_BYTES)
                    .max(1),
                retention_secs: self
                    .snapshot_retention_secs
                    .unwrap_or(SnapshotRpcConfig::DEFAULT_RETENTION_SECS),
                max_concurrent_downloads: self
                    .snapshot_max_concurrent
                    .unwrap_or(SnapshotRpcConfig::DEFAULT_MAX_CONCURRENT_DOWNLOADS)
                    .max(1),
            })
        } else {
            None
        };

        let gas_allowance = gas_limit_multiplier
            .checked_mul(node_config.block_gas_limit)
            .ok_or_else(|| {
                anyhow!(
                    "rpc gas allowance overflow: gas_limit_multiplier={gas_limit_multiplier}, block_gas_limit={}",
                    node_config.block_gas_limit
                )
            })?;

        Ok(Some(RpcConfig {
            listen_addr,
            cors,
            gas_allowance,
            chunk_size: node_config.chunk_processing_threads,
            snapshot,
        }))
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
            snapshot: self.snapshot || with.snapshot,
            snapshot_token: self.snapshot_token.or(with.snapshot_token),
            snapshot_chunk_bytes: self.snapshot_chunk_bytes.or(with.snapshot_chunk_bytes),
            snapshot_retention_secs: self
                .snapshot_retention_secs
                .or(with.snapshot_retention_secs),
            snapshot_max_concurrent: self
                .snapshot_max_concurrent
                .or(with.snapshot_max_concurrent),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_service::config::ConfigPublicKey;
    use tempfile::tempdir;

    fn node_config(block_gas_limit: u64) -> NodeConfig {
        let database_dir = tempdir().expect("temporary directory should be created");
        let key_dir = tempdir().expect("temporary directory should be created");

        NodeConfig {
            database_path: database_dir.path().to_path_buf(),
            key_path: key_dir.path().to_path_buf(),
            validator: ConfigPublicKey::Disabled,
            validator_session: ConfigPublicKey::Disabled,
            eth_max_sync_depth: 0,
            worker_threads: None,
            blocking_threads: None,
            chunk_processing_threads: 2,
            block_gas_limit,
            canonical_quarantine: 0,
            dev: false,
            pre_funded_accounts: 0,
            fast_sync: false,
            chain_deepness_threshold: 0,
        }
    }

    #[test]
    fn rejects_empty_snapshot_token() {
        let params = RpcParams {
            snapshot: true,
            snapshot_token: Some(String::new()),
            ..Default::default()
        };

        let err = params
            .into_config(&node_config(1))
            .expect_err("empty snapshot token should be rejected");
        assert!(
            err.to_string().contains("must be non-empty"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn rejects_gas_allowance_overflow() {
        let params = RpcParams {
            gas_limit_multiplier: Some(u64::MAX),
            ..Default::default()
        };

        let err = params
            .into_config(&node_config(2))
            .expect_err("gas allowance overflow should be rejected");
        assert!(
            err.to_string().contains("rpc gas allowance overflow"),
            "unexpected error: {err:#}"
        );
    }
}
