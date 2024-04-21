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

use anyhow::{anyhow, Result};
use clap::{builder::PossibleValue, Parser, ValueEnum};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use url::Url;

const CONFIG_PATH: &str = ".config/vara/config.toml";

/// Gear command line configuration
#[derive(Clone, Debug, Default, Parser, Serialize, Deserialize)]
pub struct Config {
    /// Config actions
    #[clap(subcommand)]
    #[serde(skip)]
    pub action: Action,
    /// URL for Solana's JSON RPC or moniker
    #[clap(short, long, name = "URL_OR_MONIKER", global = true)]
    pub url: Option<Network>,
}

impl Config {
    fn config() -> Result<PathBuf> {
        dirs::home_dir()
            .map(|h| h.join(CONFIG_PATH))
            .ok_or_else(|| anyhow!("Could not find config.toml from ${{HOME}}/{CONFIG_PATH}"))
    }

    /// Read the config from disk
    pub fn read(path: Option<PathBuf>) -> Result<Self> {
        let conf = path.unwrap_or(Self::config()?);
        toml::from_str(&fs::read_to_string(conf)?).map_err(Into::into)
    }

    /// Write self to disk
    pub fn write(&self, path: Option<PathBuf>) -> Result<()> {
        let conf = path.unwrap_or(Self::config()?);
        if let Some(parent) = conf.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        fs::write(conf, toml::to_string_pretty(self)?).map_err(Into::into)
    }

    /// NOTE: currently just a simple wrapper for [`Self::write`] since we
    /// just have one config option.
    pub fn exec(&self) -> Result<()> {
        if self.action == Action::Set {
            self.write(None)?
        }

        println!(
            "{}: {}",
            "RPC URL".bold(),
            self.url.clone().unwrap_or_default().as_ref()
        );
        Ok(())
    }
}

/// Config action
#[derive(Clone, Debug, Parser, PartialEq, Eq, Default)]
pub enum Action {
    /// Set a config setting
    Set,
    /// Get current config settings
    #[default]
    Get,
}

/// Vara networks
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Network {
    /// Vara main network
    #[default]
    Mainnet,
    /// Vara development network
    Testnet,
    /// Localhost endpoint
    Localhost,
    /// Customized vara network
    Custom(Url),
}

impl AsRef<str> for Network {
    fn as_ref(&self) -> &str {
        match self {
            Self::Mainnet => "wss://rpc.vara.network:443",
            Self::Testnet => "wss://testnet.vara.network:443",
            Self::Localhost => "ws://localhost:9944",
            Self::Custom(url) => url.as_str(),
        }
    }
}

impl ToString for Network {
    fn to_string(&self) -> String {
        self.as_ref().into()
    }
}

impl ValueEnum for Network {
    fn value_variants<'a>() -> &'a [Self] {
        &[Network::Mainnet, Network::Testnet, Network::Localhost]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        match self {
            Self::Mainnet => Some(PossibleValue::new("mainnet")),
            Self::Testnet => Some(PossibleValue::new("testnet")),
            Self::Localhost => Some(PossibleValue::new("localhost")),
            Self::Custom(_) => None,
        }
    }

    fn from_str(input: &str, ignore_case: bool) -> Result<Self, String> {
        Ok(match input {
            mainnet
                if mainnet == "mainnet" || ignore_case && mainnet.to_lowercase() == "mainnet" =>
            {
                Self::Mainnet
            }
            testnet
                if testnet == "testnet" || ignore_case && testnet.to_lowercase() == "testnet" =>
            {
                Self::Testnet
            }
            localhost
                if localhost == "localhost"
                    || ignore_case && localhost.to_lowercase() == "localhost" =>
            {
                Self::Localhost
            }
            _ => Self::Custom(Url::parse(input).map_err(|_| input.to_string())?),
        })
    }
}
