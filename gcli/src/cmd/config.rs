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

use clap::{builder::PossibleValue, Parser, ValueEnum};
use url::Url;

/// Gear command line configuration
#[derive(Clone, Debug, Parser)]
pub struct Config {
    /// URL for Solana's JSON RPC or moniker
    #[clap(short, long, name = "URL_OR_MONIKER")]
    pub url: Network,
}

/// Vara networks
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Network {
    /// Vara main network
    Mainnet,
    /// Vara development network
    Testnet,
    /// Localhost endpoint
    Localhost,
    /// Customized vara network
    Custom(Url),
}

impl Network {
    /// Get the RPC url from the network varianet
    pub fn to_rpc(&self) -> &str {
        match self {
            Self::Mainnet => "wss://rpc.vara.network",
            Self::Testnet => "wss://testnet.vara.network",
            Self::Localhost => "ws://localhost:9944",
            Self::Custom(url) => url.as_str(),
        }
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
