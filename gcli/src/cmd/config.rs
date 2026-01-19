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

use crate::app::App;
use anyhow::{Context, Result};
use clap::Parser;

use colored::Colorize;
use gsdk::Api;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf, str::FromStr};
use url::Url;

const CONFIG_PATH: &str = "gear-gcli/config.toml";

/// Access `gcli` persistent configuration.
#[derive(Clone, Debug, Parser)]
pub struct Config {
    #[clap(subcommand)]
    action: Action,
}

impl Config {
    /// NOTE: currently just a simple wrapper for [`ConfigSettings::write`]
    /// since we just have one config option.
    pub fn exec(self, app: &mut App) -> Result<()> {
        let mut config = app.config()?;

        match self.action {
            Action::Set(option) => {
                config.set(option);
                config
                    .write()
                    .context("failed to write new configuration")?;

                println!("Successfully updated the configuration");
                println!();
                config.pretty_print();
            }
            Action::Get => {
                app.config()?.pretty_print();
            }
            Action::Reset => {
                config = ConfigSettings::default();
                config
                    .write()
                    .context("failed to write new configuration")?;

                println!("Successfully reset the configuration");
                println!();
                config.pretty_print();
            }
        }

        Ok(())
    }
}

/// `gcli` persistent configuration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ConfigSettings {
    /// Gear node RPC endpoint.
    ///
    /// Can be `mainnet`, `testnest`, `localhost` or a custom URL.
    #[serde(alias = "url")]
    pub endpoint: Endpoint,
}

#[derive(Debug, Clone, Parser)]
enum ConfigOption {
    /// Sets the default RPC endpoint.
    Endpoint {
        /// New default RPC endpoint.
        endpoint: Endpoint,
    },
}

impl ConfigSettings {
    fn config_path() -> Result<PathBuf> {
        Ok(if cfg!(test) {
            env::temp_dir().join("gcli-test").join("config")
        } else {
            dirs::config_dir()
                .context("failed to get config directory")?
                .join(CONFIG_PATH)
        })
    }

    /// Reads the configuration from disk.
    pub fn read() -> Result<ConfigSettings> {
        let path = Self::config_path()?;

        if path.exists() {
            let contents = fs::read_to_string(path)?;

            Ok(toml::from_str(&contents)?)
        } else {
            Ok(Self::default())
        }
    }

    /// Sets the configuration option.
    fn set(&mut self, option: ConfigOption) {
        match option {
            ConfigOption::Endpoint { endpoint } => self.endpoint = endpoint,
        }
    }

    /// Writes the configuration to disk.
    pub fn write(&self) -> Result<()> {
        let path = Self::config_path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)?;

        Ok(fs::write(path, contents)?)
    }

    /// Pretty-prints the configuration.
    pub fn pretty_print(&self) {
        println!("{} {}", "RPC URL:".bold(), self.endpoint.as_str())
    }
}

/// Config action
#[derive(Clone, Debug, Parser)]
enum Action {
    /// Set a persistent option.
    #[clap(subcommand)]
    Set(ConfigOption),
    /// Print current configuration.
    Get,
    /// Reset the persistent configuration.
    Reset,
}

/// Vara networks
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Endpoint {
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

impl Endpoint {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Mainnet => Api::VARA_ENDPOINT,
            Self::Testnet => Api::VARA_TESTNET_ENDPOINT,
            Self::Localhost => Api::DEV_ENDPOINT,
            Self::Custom(url) => url.as_str(),
        }
    }
}

impl FromStr for Endpoint {
    type Err = url::ParseError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "mainnet" => Self::Mainnet,
            "testnet" => Self::Testnet,
            "localhost" => Self::Localhost,
            input => Self::Custom(Url::parse(input)?),
        })
    }
}
