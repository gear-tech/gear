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

//! Node-scoped parameters shared across `run`, `key`, `tx`, and `check`.

use super::MergeParams;
use anyhow::{Context, Result, ensure};
use clap::Parser;
use directories::ProjectDirs;
use ethexe_common::{
    DEFAULT_BLOCK_GAS_LIMIT,
    consensus::{DEFAULT_BATCH_SIZE_LIMIT, DEFAULT_CHAIN_DEEPNESS_THRESHOLD, MAX_BATCH_SIZE_LIMIT},
    gear::{CANONICAL_QUARANTINE, MAX_BLOCK_GAS_LIMIT},
};
use ethexe_processor::DEFAULT_CHUNK_SIZE;
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
    pub base: Option<PathBuf>,

    /// Flag to use temporary directory for database.
    #[arg(long)]
    #[serde(default)]
    pub tmp: bool,

    /// Flag to run node in development mode.
    #[arg(long)]
    #[serde(default)]
    pub dev: bool,

    /// Number of pre-funded accounts to generate in dev mode.
    #[arg(long)]
    #[serde(rename = "pre-funded-accounts")]
    pub pre_funded_accounts: Option<NonZero<u32>>,

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

    /// Batch size limit for the node.
    #[arg(long)]
    #[serde(rename = "batch-size-limit")]
    pub batch_size_limit: Option<u64>,

    /// Quarantine for canonical (Ethereum) messages.
    #[arg(long)]
    #[serde(rename = "canonical-quarantine")]
    pub canonical_quarantine: Option<u8>,

    /// Do P2P database synchronization before the main loop
    #[arg(long, default_value = "false")]
    #[serde(default, rename = "fast-sync")]
    pub fast_sync: bool,

    /// Threshold for producer to submit commitment despite of no transitions
    #[arg(long)]
    #[serde(default, rename = "chain-deepness-threshold")]
    pub chain_deepness_threshold: Option<u32>,

    /// Path to genesis state dump file (.blob or .json) for initial chain state.
    #[arg(long)]
    #[serde(default, rename = "genesis-state-dump")]
    pub genesis_state_dump: Option<PathBuf>,
}

impl NodeParams {
    /// Default max allowed height diff from head for sync directly from Ethereum.
    pub const DEFAULT_MAX_DEPTH: NonZero<u32> = NonZero::new(100_000).unwrap();
    /// Default number of pre-funded accounts in dev mode.
    pub const DEFAULT_PRE_FUNDED_ACCOUNTS: NonZero<u32> = NonZero::new(10).unwrap();

    /// Converts CLI/TOML node parameters into a service-ready [`NodeConfig`].
    ///
    /// Besides simple field mapping this also:
    /// - validates that validator and validator-session are configured together
    /// - resolves the effective database and key directories
    /// - clamps gas and batch limits to protocol maxima
    /// - fills in defaults for the execution and sync knobs
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
                .unwrap_or(DEFAULT_CHUNK_SIZE)
                .get(),
            block_gas_limit: self
                .block_gas_limit
                .unwrap_or(DEFAULT_BLOCK_GAS_LIMIT)
                .min(MAX_BLOCK_GAS_LIMIT),
            batch_size_limit: self
                .batch_size_limit
                .unwrap_or(DEFAULT_BATCH_SIZE_LIMIT)
                .min(MAX_BATCH_SIZE_LIMIT),
            canonical_quarantine: self.canonical_quarantine.unwrap_or(CANONICAL_QUARANTINE),
            dev: self.dev,
            pre_funded_accounts: self
                .pre_funded_accounts
                .unwrap_or(Self::DEFAULT_PRE_FUNDED_ACCOUNTS)
                .get(),
            fast_sync: self.fast_sync,
            chain_deepness_threshold: self
                .chain_deepness_threshold
                .unwrap_or(DEFAULT_CHAIN_DEEPNESS_THRESHOLD),
            genesis_state_dump: self.genesis_state_dump,
        })
    }

    /// Returns the database directory used by RocksDB.
    ///
    /// Development and temporary modes intentionally keep the database in a fresh temp
    /// directory so that local experiments do not reuse persistent state.
    pub fn db_dir(&self) -> PathBuf {
        if self.tmp || self.dev {
            Self::tmp_db()
        } else {
            self.base().join("db")
        }
    }

    /// Returns the directory that stores validator and sender keys.
    pub fn keys_dir(&self) -> PathBuf {
        self.base().join("keys")
    }

    /// Returns the directory that stores the libp2p identity.
    pub fn net_dir(&self) -> PathBuf {
        self.base().join("net")
    }

    fn base(&self) -> PathBuf {
        self.base.clone().unwrap_or_else(Self::default_base)
    }

    /// Computes the platform-specific default base directory.
    fn default_base() -> PathBuf {
        ProjectDirs::from("com", "Gear", "ethexe")
            .expect("couldn't find home directory")
            .data_dir()
            .to_path_buf()
    }

    /// Lazily allocates and returns a process-wide temporary database directory.
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
            dev: self.dev || with.dev,

            pre_funded_accounts: self.pre_funded_accounts.or(with.pre_funded_accounts),
            validator: self.validator.or(with.validator),
            validator_session: self.validator_session.or(with.validator_session),

            max_depth: self.max_depth.or(with.max_depth),

            worker_threads: self.worker_threads.or(with.worker_threads),
            blocking_threads: self.blocking_threads.or(with.blocking_threads),
            chunk_processing_threads: self
                .chunk_processing_threads
                .or(with.chunk_processing_threads),

            block_gas_limit: self.block_gas_limit.or(with.block_gas_limit),
            batch_size_limit: self.batch_size_limit.or(with.batch_size_limit),
            canonical_quarantine: self.canonical_quarantine.or(with.canonical_quarantine),

            fast_sync: self.fast_sync || with.fast_sync,

            chain_deepness_threshold: self
                .chain_deepness_threshold
                .or(with.chain_deepness_threshold),

            genesis_state_dump: self.genesis_state_dump.or(with.genesis_state_dump),
        }
    }
}
