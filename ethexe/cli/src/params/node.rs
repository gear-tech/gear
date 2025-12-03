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
use anyhow::{Context, Result, ensure};
use clap::Parser;
use directories::ProjectDirs;
use ethexe_common::{
    DEFAULT_BLOCK_GAS_LIMIT,
    gear::{CANONICAL_QUARANTINE, MAX_BLOCK_GAS_LIMIT},
};
use ethexe_processor::DEFAULT_CHUNK_PROCESSING_THREADS;
use ethexe_service::config::{ConfigPublicKey, NodeConfig};
use serde::Deserialize;
use std::{num::NonZero, path::PathBuf};
use tempfile::TempDir;

#[static_init::dynamic(drop, lazy)]
static mut TMP_DB: Option<TempDir> = None;

/// General node-describing parameters, responsible for its roles and execution configuration.
#[derive(Clone, Debug, Default, Deserialize, Parser)]
#[serde(deny_unknown_fields)]
pub struct NodeParams {
    /// Base directory for all node-related subdirectories.
    #[arg(long)]
    pub base: Option<String>,

    /// Flag to use temporary directory for database.
    #[arg(long)]
    #[serde(default)]
    pub tmp: bool,

    /// Public key of the validator, if node should act as one.
    #[arg(long)]
    pub validator: Option<String>,

    /// Public key of the validator session, if node should act as one.
    #[arg(long)]
    #[serde(rename = "validator-session")]
    pub validator_session: Option<String>,

    /// Max allowed height diff from head for sync directly from Ethereum.
    #[arg(long)]
    #[serde(rename = "max-depth")]
    pub max_depth: Option<NonZero<u32>>,

    /// Number of worker threads to use in tokio runtime.
    #[arg(long)]
    #[serde(rename = "worker-threads")]
    pub worker_threads: Option<NonZero<usize>>,

    /// Number of blocking threads to use in tokio runtime.
    #[arg(long)]
    #[serde(rename = "blocking-threads")]
    pub blocking_threads: Option<NonZero<usize>>,

    /// Number of threads to use for chunk processing.
    #[arg(long)]
    #[serde(rename = "chunk-processing-threads")]
    pub chunk_processing_threads: Option<NonZero<usize>>,

    /// Block gas limit for the node.
    #[arg(long)]
    #[serde(rename = "block-gas-limit")]
    pub block_gas_limit: Option<u64>,

    #[arg(long)]
    #[serde(rename = "canonical-quarantine")]
    pub canonical_quarantine: Option<u8>,

    /// Do P2P database synchronization before the main loop
    #[arg(long, default_value = "false")]
    #[serde(default, rename = "fast-sync")]
    pub fast_sync: bool,

    /// Limit for validating chain deepness of coming commitments.
    #[arg(long, default_value = "20000")]
    #[serde(default, rename = "validate-chain-deepness-limit")]
    pub validate_chain_deepness_limit: u32,

    /// Threshold for producer to submit commitment despite of no transitions
    #[arg(long, default_value = "500")]
    #[serde(default, rename = "chain-deepness-threshold")]
    pub chain_deepness_threshold: u32,
}

impl NodeParams {
    /// Default max allowed height diff from head for sync directly from Ethereum.
    pub const DEFAULT_MAX_DEPTH: NonZero<u32> = NonZero::new(100_000).unwrap();

    /// Convert self into a proper `NodeConfig` object.
    pub fn into_config(self) -> Result<NodeConfig> {
        ensure!(
            self.validator.is_some() == self.validator_session.is_some(),
            "`validator` and `validator-session` must be both set or both unset"
        );

        Ok(NodeConfig {
            database_path: self.db_dir(),
            key_path: self.keys_dir(),
            validator: ConfigPublicKey::new(&self.validator)
                .with_context(|| "invalid `validator` key")?,
            validator_session: ConfigPublicKey::new(&self.validator_session)
                .with_context(|| "invalid `validator-session` key")?,
            eth_max_sync_depth: self.max_depth.unwrap_or(Self::DEFAULT_MAX_DEPTH).get(),
            worker_threads: self.worker_threads.map(|v| v.get()),
            blocking_threads: self.blocking_threads.map(|v| v.get()),
            chunk_processing_threads: self
                .chunk_processing_threads
                .unwrap_or(DEFAULT_CHUNK_PROCESSING_THREADS)
                .get(),
            block_gas_limit: self
                .block_gas_limit
                .unwrap_or(DEFAULT_BLOCK_GAS_LIMIT)
                .min(MAX_BLOCK_GAS_LIMIT),
            canonical_quarantine: self.canonical_quarantine.unwrap_or(CANONICAL_QUARANTINE),
            fast_sync: self.fast_sync,
            validate_chain_deepness_limit: self.validate_chain_deepness_limit,
            chain_deepness_threshold: self.chain_deepness_threshold,
        })
    }

    /// Get path to the database directory.
    pub fn db_dir(&self) -> PathBuf {
        if self.tmp {
            Self::tmp_db()
        } else {
            self.base().join("db")
        }
    }

    /// Get path to the keystore directory.
    pub fn keys_dir(&self) -> PathBuf {
        self.base().join("keys")
    }

    /// Get path to the network directory.
    pub fn net_dir(&self) -> PathBuf {
        self.base().join("net")
    }

    fn base(&self) -> PathBuf {
        self.base
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(Self::default_base)
    }

    fn default_base() -> PathBuf {
        ProjectDirs::from("com", "Gear", "ethexe")
            .expect("couldn't find home directory")
            .data_dir()
            .to_path_buf()
    }

    fn tmp_db() -> PathBuf {
        let mut tmp = TMP_DB.write();

        if tmp.is_none() {
            *tmp = Some(
                tempfile::Builder::new()
                    .prefix("ethexe")
                    .tempdir()
                    .expect("couldn't create temp dir"),
            );
        }

        tmp.as_ref().expect("infallible; set above").path().into()
    }
}

impl MergeParams for NodeParams {
    fn merge(self, with: Self) -> Self {
        Self {
            base: self.base.or(with.base),
            tmp: self.tmp || with.tmp,

            validator: self.validator.or(with.validator),
            validator_session: self.validator_session.or(with.validator_session),

            max_depth: self.max_depth.or(with.max_depth),

            worker_threads: self.worker_threads.or(with.worker_threads),
            blocking_threads: self.blocking_threads.or(with.blocking_threads),
            chunk_processing_threads: self
                .chunk_processing_threads
                .or(with.chunk_processing_threads),

            block_gas_limit: self.block_gas_limit.or(with.block_gas_limit),
            canonical_quarantine: self.canonical_quarantine.or(with.canonical_quarantine),

            fast_sync: self.fast_sync || with.fast_sync,
            validate_chain_deepness_limit: self.validate_chain_deepness_limit,
            chain_deepness_threshold: self.chain_deepness_threshold,
        }
    }
}
