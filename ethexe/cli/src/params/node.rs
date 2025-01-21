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
use directories::ProjectDirs;
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

    /// Flag to run node in development mode.
    #[arg(long)]
    #[serde(default)]
    pub dev: bool,

    /// Public key of the sequencer, if node should act as one.
    #[arg(long)]
    pub sequencer: Option<String>,

    /// Public key of the validator, if node should act as one.
    #[arg(long)]
    pub validator: Option<String>,

    /// Max allowed height diff from head for sync directly from Ethereum.
    #[arg(long)]
    #[serde(rename = "max-depth")]
    pub max_depth: Option<NonZero<u32>>,

    /// Number of physical threads to use.
    #[arg(long)]
    #[serde(rename = "physical-threads")]
    pub physical_threads: Option<NonZero<u8>>,

    /// Number of virtual thread to use for programs processing.
    #[arg(long)]
    #[serde(rename = "virtual-threads")]
    pub virtual_threads: Option<NonZero<u8>>,
}

impl NodeParams {
    /// Default max allowed height diff from head for sync directly from Ethereum.
    pub const DEFAULT_MAX_DEPTH: NonZero<u32> = unsafe { NonZero::new_unchecked(100_000) };

    /// Default amount of virtual threads to use for programs processing.
    pub const DEFAULT_VIRTUAL_THREADS: NonZero<u8> = unsafe { NonZero::new_unchecked(16) };

    /// Convert self into a proper `NodeConfig` object.
    pub fn into_config(self) -> Result<NodeConfig> {
        Ok(NodeConfig {
            database_path: self.db_dir(),
            key_path: self.keys_dir(),
            sequencer: ConfigPublicKey::new(&self.sequencer)
                .with_context(|| "invalid `sequencer` key")?,
            validator: ConfigPublicKey::new(&self.validator)
                .with_context(|| "invalid `validator` key")?,
            max_commitment_depth: self.max_depth.unwrap_or(Self::DEFAULT_MAX_DEPTH).get(),
            worker_threads_override: self.physical_threads.map(|v| v.get() as usize),
            virtual_threads: self
                .virtual_threads
                .unwrap_or(Self::DEFAULT_VIRTUAL_THREADS)
                .get() as usize,
            dev: self.dev,
        })
    }

    /// Get path to the database directory.
    pub fn db_dir(&self) -> PathBuf {
        if self.tmp || self.dev {
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
            dev: self.dev || with.dev,
            sequencer: self.sequencer.or(with.sequencer),
            validator: self.validator.or(with.validator),

            max_depth: self.max_depth.or(with.max_depth),

            physical_threads: self.physical_threads.or(with.physical_threads),
            virtual_threads: self.virtual_threads.or(with.virtual_threads),
        }
    }
}
