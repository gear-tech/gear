// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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
use anyhow::{Result, anyhow};
use clap::Parser;
use colored::Colorize;
use gsdk::AccountKeyring;
use gsigner::{
    cli::{GSignerCommands, display_result, execute_command},
    keyring::KeystoreEntry,
    sr25519::PrivateKey,
};

const DEFAULT_DEV: AccountKeyring = AccountKeyring::Alice;

/// Gear wallet manager.
#[derive(Clone, Debug, Parser)]
pub enum Wallet {
    /// Switch to development account
    Dev {
        /// The name of the dev account.
        #[clap(short, long, default_value = "_dev_alice")]
        name: String,
        /// The URI of the dev account.
        #[clap(short, long)]
        uri: Option<String>,
    },
    /// gsigner commands embedded into gcli.
    #[clap(flatten)]
    Signer(GSignerCommands),
}

impl Wallet {
    /// Run the wallet command.
    pub fn exec(self, app: &mut App) -> Result<()> {
        match self {
            Wallet::Dev { name, uri } => Self::dev(&name, uri.clone(), app),
            Wallet::Signer(command) => {
                let result = execute_command(command.clone())?;
                display_result(&result);
                Ok(())
            }
        }
    }

    /// Switch to development account.
    pub fn dev(name: &str, uri: Option<String>, app: &mut App) -> Result<()> {
        let mut keyring = app.keyring()?;

        if !keyring
            .list()
            .iter()
            .any(|keystore| keystore.name() == name)
        {
            let suri = uri.unwrap_or_else(|| DEFAULT_DEV.to_string());
            let private_key = PrivateKey::from_suri(&suri, None)
                .map_err(|e| anyhow!("Failed to create keypair from the input uri: {e}"))?;
            keyring.add(name, private_key, None)?;
        }

        keyring.set_primary(name)?;
        println!("Successfully switched to dev account {} !", name.cyan());
        Ok(())
    }
}
