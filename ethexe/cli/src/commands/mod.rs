// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Executable command handlers for the `ethexe` CLI.
//!
//! Each submodule owns one command family and is responsible for merging file-backed
//! configuration into its command-line arguments before execution.

use crate::params::Params;
use anyhow::Result;
use clap::Subcommand;

mod check;
mod dump;
mod key;
mod malachite;
mod run;
mod tx;

pub use check::CheckCommand;
pub use dump::DumpCommand;
pub use key::KeyCommand;
pub use malachite::MalachiteCommand;
pub use run::RunCommand;
pub use tx::TxCommand;

/// CLI command.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Keystore manipulations.
    Key(KeyCommand),
    /// Run the node.
    Run(RunCommand),
    /// Submit a transaction.
    Tx(TxCommand),
    /// Check ethexe database for integrity and/or computation correctness.
    /// By default start all checks.
    /// By default, progress bar is enabled, use `--verbose` to enable debug logging and disable progress bar.
    Check(CheckCommand),
    /// State dump operations for re-genesis.
    Dump(DumpCommand),
    /// Malachite-specific helper commands (peer-id derivation, etc.).
    Malachite(MalachiteCommand),
}

impl Command {
    /// Merge the command with the provided params.
    fn with_file_params(self, file_params: Params) -> Self {
        match self {
            Self::Key(key_cmd) => Self::Key(key_cmd.with_params(file_params)),
            Self::Run(run_cmd) => Self::Run(run_cmd.with_params(file_params)),
            Self::Tx(tx_cmd) => Self::Tx(tx_cmd.with_params(file_params)),
            Self::Check(check_cmd) => Self::Check(check_cmd.with_params(file_params)),
            Self::Dump(dump_cmd) => Self::Dump(dump_cmd.with_params(file_params)),
            Self::Malachite(mala_cmd) => Self::Malachite(mala_cmd.with_params(file_params)),
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
            Command::Dump(dump_cmd) => dump_cmd.exec(),
            Command::Malachite(mala_cmd) => mala_cmd.exec(),
        }
    }
}
