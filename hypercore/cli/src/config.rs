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

//! Cargo extension for building gear programs.

use crate::args::Args;

use anyhow::{Context as _, Result};
use directories::ProjectDirs;
use std::path::PathBuf;

pub struct Config {
    /// RPC of the Ethereum endpoint
    pub ethereum_rpc: String,

    /// Path of the state database
    pub database_path: PathBuf,

    /// Path of the network configuration (keys and peers)
    pub network_path: PathBuf,

    /// Signer key storage path
    pub key_path: PathBuf,
}

impl TryFrom<Args> for Config {
    type Error = anyhow::Error;

    fn try_from(args: Args) -> Result<Self> {
        let base_path = match args.base_path {
            Some(path) => path,
            None => {
                let proj_dirs = ProjectDirs::from("com", "Gear", "Hypercore")
                    .with_context(|| "Invalid home directory path")?;
                proj_dirs.config_dir().to_path_buf()
            }
        };

        Ok(Config {
            ethereum_rpc: args.ethereum_rpc,
            database_path: base_path.join("db"),
            network_path: base_path.join("net"),
            key_path: base_path.join("key"),
        })
    }
}
