// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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
use clap::Parser;
use gring::{cmd::Command, Keyring, SecretKey};
use gsdk::ext::sp_core::{sr25519, Pair};

const DEFAULT_DEV: &str = "//Alice";

/// Wallet command
#[derive(Clone, Debug, Parser)]
pub enum Wallet {
    /// Switch to development account
    Dev {
        /// The name of the dev account.
        name: String,
        /// The URI of the dev account.
        uri: Option<String>,
    },
    /// Flatted gring command
    #[clap(flatten)]
    Gring(Command),
}

impl Wallet {
    pub fn run(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Switch to development account.
    pub fn dev(name: &str, uri: Option<String>) -> Result<()> {
        let mut keyring = Keyring::load(Command::store()?)?;
        if keyring.set_primary(name.into()).is_ok() {
            return Ok(());
        }

        let sk = SecretKey::from_bytes(
            &sr25519::Pair::from_string(&uri.unwrap_or(DEFAULT_DEV.into()), None)?.to_raw_vec(),
        )
        .map_err(|_| anyhow!("Faild to create keypair from the input uri."))?;

        keyring.add(name, sk.into(), None)?;
        keyring.set_primary(name.into())?;
        Ok(())
    }
}
