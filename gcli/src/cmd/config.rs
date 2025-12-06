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

use clap::{Parser, ValueEnum, builder::PossibleValue};
use color_eyre::{Result, eyre::eyre};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt, fs, path::PathBuf};
use url::Url;

const CONFIG_PATH: &str = ".config/vara/config.toml";

/// Gear command line configuration
#[derive(Clone, Debug, Parser)]
pub struct Config {
    /// Config actions
    #[clap(subcommand)]
    pub action: Action,
}

impl Config {
    /// NOTE: currently just a simple wrapper for [`ConfigSettings::write`]
    /// since we just have one config option.
    pub fn exec(&self) -> Result<()> {
        match &self.action {
            Action::Set(s) => {
                s.write(None)?;
                println!("{s}");
            }
            // prints the whole config atm.
            Action::Get { url: _ } => {
                let settings = ConfigSettings::read(None)?;
                println!("{settings}");
            }
        }

        Ok(())
    }
}

/// Gear command client config settings
#[derive(Clone, Debug, Parser, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ConfigSettings {
    /// URL for Vara's JSON RPC or moniker
    #[clap(short, long, name = "URL_OR_MONIKER")]
    pub url: Network,
}

impl ConfigSettings {
    fn config() -> Result<PathBuf> {
        dirs::home_dir()
            .map(|h| h.join(CONFIG_PATH))
            .ok_or_else(|| eyre!("Could not find config.toml from ${{HOME}}/{CONFIG_PATH}"))
    }

    /// Read the config from disk
    pub fn read(path: Option<PathBuf>) -> Result<ConfigSettings> {
        let conf = path.unwrap_or(Self::config()?);
        toml::from_str(&fs::read_to_string(conf)?).map_err(Into::into)
    }

    /// Write the whole settings to disk
    ///
    /// NOTE: this method should be updated as well once
    /// there are more options in the settings.
    pub fn write(&self, path: Option<PathBuf>) -> Result<()> {
        let conf = path.unwrap_or(Self::config()?);

        if let Some(parent) = conf.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)?;
        }

        fs::write(conf, toml::to_string_pretty(self)?).map_err(Into::into)
    }
}

impl fmt::Display for ConfigSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", "RPC URL".bold(), self.url.clone().as_ref())
    }
}

/// Config action
#[derive(Clone, Debug, Parser, PartialEq, Eq)]
pub enum Action {
    /// Set a config setting
    Set(ConfigSettings),
    /// Get current config settings
    Get {
        /// Get the rpc url from the current config settings.
        #[clap(short, long)]
        url: bool,
    },
}

/// Vara networks
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
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
        let input = if ignore_case {
            Cow::Owned(input.to_lowercase())
        } else {
            Cow::Borrowed(input)
        };

        Ok(match input.as_ref() {
            "mainnet" => Self::Mainnet,
            "testnet" => Self::Testnet,
            "localhost" => Self::Localhost,
            input => Self::Custom(Url::parse(input).map_err(|_| input.to_string())?),
        })
    }
}
