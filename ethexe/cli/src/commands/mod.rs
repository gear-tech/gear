// This file is part of Gear.
//
// Copyright (C) 2024-2026 Gear Technologies Inc.
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
use clap::Subcommand;

mod check;
mod key;
mod run;
mod tx;

pub use check::CheckCommand;
pub use key::KeyCommand;
pub use run::RunCommand;
pub use tx::TxCommand;

/// CLI command.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Keystore manipulations.
    Key(KeyCommand),
    /// Run the node.
    Run(Box<RunCommand>),
    /// Submit a transaction.
    Tx(TxCommand),
    /// Check ethexe database for integrity and/or computation correctness.
    /// By default start all checks.
    /// By default, progress bar is enabled, use `--verbose` to enable debug logging and disable progress bar.
    Check(CheckCommand),
}

impl Command {
    /// Merge the command with the provided params.
    fn with_file_params(self, file_params: Params) -> Self {
        match self {
            Self::Key(key_cmd) => Self::Key(key_cmd.with_params(file_params)),
            Self::Run(run_cmd) => Self::Run(Box::new(run_cmd.with_params(file_params))),
            Self::Tx(tx_cmd) => Self::Tx(tx_cmd.with_params(file_params)),
            Self::Check(check_cmd) => Self::Check(check_cmd),
        }
    }

    /// Run the command.
    pub fn run(self, file_params: Params) -> Result<()> {
        let cmd = self.with_file_params(file_params);

        match cmd {
            Command::Key(key_cmd) => key_cmd.exec(),
            Command::Tx(tx_cmd) => tx_cmd.exec(),
            Command::Run(run_cmd) => run_cmd.run(),
            Command::Check(check_cmd) => check_cmd.exec(),
        }
    }
}
