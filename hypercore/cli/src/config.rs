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

//! Application config in one place.

use crate::args::Args;

use anyhow::{Context as _, Result};
use directories::ProjectDirs;
use hypercore_network::NetworkConfiguration;
use std::path::PathBuf;

#[derive(Default)]
pub enum SequencerConfig {
    Enabled(String),
    #[default]
    Disabled,
}

#[derive(Default)]
pub enum ValidatorConfig {
    Enabled(String),
    #[default]
    Disabled,
}

pub struct Config {
    /// RPC of the Ethereum endpoint
    pub ethereum_rpc: String,

    /// RPC of the Ethereum Beacon endpoint
    pub ethereum_beacon_rpc: String,

    /// Address of Ethereum Router contract
    pub ethereum_router_address: String,

    /// Address of Ethereum Program contract
    pub ethereum_program_address: String,

    /// Network path
    pub network_path: PathBuf,

    /// Path of the state database
    pub database_path: PathBuf,

    /// Signer key storage path
    pub key_path: PathBuf,

    /// Is this role a sequencer
    pub sequencer: SequencerConfig,

    /// Is this role a validator
    pub validator: ValidatorConfig,

    // Network configuration
    pub net_config: NetworkConfiguration,
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
            ethereum_beacon_rpc: args.ethereum_beacon_rpc,
            ethereum_router_address: args.ethereum_router_address,
            ethereum_program_address: args.ethereum_program_address,
            net_config: args.network_params.network_config(
                Some(base_path.join("net")),
                "test",
                Default::default(),
                hypercore_network::DEFAULT_LISTEN_PORT,
            ),
            database_path: base_path.join("db"),
            network_path: base_path.join("net"),
            key_path: base_path.join("key"),
            sequencer: match args.sequencer_key {
                Some(key) => SequencerConfig::Enabled(key),
                None => SequencerConfig::Disabled,
            },
            validator: match args.validator_key {
                Some(key) => ValidatorConfig::Enabled(key),
                None => ValidatorConfig::Disabled,
            },
        })
    }
}
