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

//! CLI implementation for gring.
#![cfg(feature = "cli")]

use crate::Keyring;
use anyhow::{anyhow, Result};
use clap::Parser;
use colored::Colorize;
use std::{fs, path::PathBuf};

/// gring sub commands.
#[derive(Parser)]
pub enum Command {
    /// Generate a new key.
    Generate {
        /// The name of the key.
        name: String,
        /// The passphrase of the key.
        #[arg(short, long)]
        passphrase: String,
        /// If the key should be a vanity key.
        #[arg(short, long)]
        vanity: Option<String>,
    },
    /// List all keys in keystore.
    List,
}

impl Command {
    /// The path of the keyring store.
    ///
    /// NOTE: This is currently not configurable.
    pub fn store() -> Result<PathBuf> {
        let app = env!("CARGO_PKG_NAME");
        let store = dirs::data_dir()
            .ok_or_else(|| anyhow!("Failed to locate app directory."))?
            .join(app);

        fs::create_dir_all(&store).map_err(|e| {
            tracing::error!("Failed to create keyring store at {store:?}, {e}");
            e
        })?;

        tracing::info!(
            "keyring store: {}",
            store.display().to_string().underline().dimmed()
        );
        Ok(store)
    }

    /// Run the command.
    pub fn run(self) -> Result<()> {
        let mut keyring = Keyring::load(Command::store()?)?;
        match self {
            Command::Generate {
                name,
                vanity,
                passphrase,
            } => {
                if name.len() > 16 {
                    return Err(anyhow!("Name must be less than 16 characters."));
                }

                let (keystore, keypair) =
                    keyring.create(&name, vanity.as_deref(), Some(passphrase.as_ref()))?;
                let path = keyring.store.join(name).with_extension("json");
                println!("VARA Address: {}", keystore.address.to_string());
                println!("Public Key:   0x{}", hex::encode(keypair.public));
                println!(
                    "Drag {} to the polkadot.js extension to import it.",
                    path.display().to_string().underline()
                );
            }
            Command::List => {
                println!("| {:<16} | {:<49} |", "Name".bold(), "Address".bold());
                println!("| {} | {} |", "-".repeat(16), "-".repeat(49));
                for key in keyring.list() {
                    println!(
                        "| {:<16} | {} |",
                        key.meta.name,
                        key.address.to_string().cyan()
                    );
                }
            }
        }
        Ok(())
    }
}
