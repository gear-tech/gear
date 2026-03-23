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

use crate::params::Params;
use anyhow::Result;
use clap::Parser;
use gsigner::cli::{
    SchemeCommands, SchemeKeyringCommands, SchemeSubcommand, display_result, execute_command,
};
use std::path::PathBuf;

/// Keystore manipulations.
#[derive(Debug, Parser)]
pub struct KeyCommand {
    /// Primary key store to use (use to override generation from base path).
    #[arg(short, long)]
    pub key_store: Option<PathBuf>,

    /// Use network key store.
    #[arg(long = "net", default_value = "false")]
    pub network: bool,

    /// Subcommand to run.
    #[command(subcommand)]
    pub command: SchemeSubcommand,
}

impl KeyCommand {
    /// Merge the command with the provided params.
    pub fn with_params(mut self, params: Params) -> Self {
        let node = params.node.unwrap_or_default();

        self.key_store = self.key_store.take().or_else(|| {
            if self.network {
                Some(node.net_dir())
            } else {
                Some(node.keys_dir())
            }
        });

        self
    }

    /// Execute the command.
    pub fn exec(self) -> Result<()> {
        let key_store = self.key_store.expect("must never be empty after merging");

        let command = apply_default_storage(self.command, key_store);
        let result = execute_command(SchemeCommands::Secp256k1 { command })?;
        display_result(&result);

        Ok(())
    }
}

fn apply_default_storage(command: SchemeSubcommand, default: PathBuf) -> SchemeSubcommand {
    match command {
        SchemeSubcommand::Keyring { mut command } => {
            apply_default_storage_keyring(&mut command, &default);
            SchemeSubcommand::Keyring { command }
        }
        other => other,
    }
}

fn apply_default_storage_keyring(command: &mut SchemeKeyringCommands, default: &std::path::Path) {
    match command {
        SchemeKeyringCommands::Clear { storage } | SchemeKeyringCommands::List { storage } => {
            if storage.path.is_none() && !storage.memory {
                storage.path = Some(default.to_path_buf());
            }
        }
        SchemeKeyringCommands::Generate { storage, .. }
        | SchemeKeyringCommands::Import { storage, .. }
        | SchemeKeyringCommands::Sign { storage, .. }
        | SchemeKeyringCommands::Show { storage, .. }
        | SchemeKeyringCommands::Init { storage }
        | SchemeKeyringCommands::Create { storage, .. }
        | SchemeKeyringCommands::Vanity { storage, .. } => {
            if storage.path.is_none() && !storage.memory {
                storage.path = Some(default.to_path_buf());
            }
        }
        _ => {}
    }
}
