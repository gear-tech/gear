// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # ethexe-cli
//!
//! Command-line entrypoint for operating Vara.eth (ethexe) nodes. This crate contains no
//! business logic: it parses arguments, loads configuration, merges file-based config with CLI
//! flags (CLI wins), and delegates all real work to the underlying service and library crates.
//!
//! ## Role in the Stack
//!
//! `ethexe-cli` is a leaf binary at the top of the ethexe workspace; no other ethexe crate
//! depends on it. It builds an `ethexe_service::config::Config` and calls into `ethexe-service`
//! for the `run` subcommand.
//!
//! ## Entry Point
//!
//! [`Cli`] is the only public item exported from the crate. [`Cli::run`] is the single entry
//! point, and the default config path is [`Cli::DEFAULT_PARAMS_PATH`].
//!
//! ```rust,no_run
//! use clap::Parser;
//! use ethexe_cli::Cli;
//!
//! fn main() -> anyhow::Result<()> {
//!     Cli::parse().run()
//! }
//! ```
//!
//! ## Command Groups
//!
//! | Subcommand  | Purpose                                              |
//! |-------------|------------------------------------------------------|
//! | `run`       | Launch the full ethexe service stack                 |
//! | `key`       | Keystore manipulation (generate, inspect keypairs)   |
//! | `tx`        | Submit Ethereum and injected transactions            |
//! | `check`     | Verify the ethexe database for integrity/correctness |
//! | `dump`      | State dump operations for re-genesis                 |
//! | `malachite` | Malachite-consensus helpers (e.g. peer-id derivation)|

use anyhow::{Context, Result};
use clap::Parser;
use commands::Command;
use params::Params;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

mod commands;
mod params;
mod utils;

/// Returns the crate version suffixed with the short git SHA injected by `build.rs`.
fn version() -> &'static str {
    concat!(env!("CARGO_PKG_VERSION"), "-", env!("GIT_SHA"))
}

/// Top-level command-line interface for the `ethexe` binary.
#[derive(Debug, Parser)]
#[command(name = "ethexe", version = version())]
pub struct Cli {
    /// Path to the TOML config file. If not provided, the default path "./.ethexe.toml" is used. To disable parsing of the config file, use "none".
    #[arg(long)]
    pub cfg: Option<String>,

    /// Command to run.
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    /// Default path to the TOML config file.
    pub const DEFAULT_PARAMS_PATH: &str = "./.ethexe.toml";

    /// Run the CLI.
    pub fn run(self) -> Result<()> {
        let params = self
            .file_params()
            .with_context(|| "failed to read params from file")?
            .unwrap_or_default();

        self.command.run(params)
    }

    fn file_params(&self) -> Result<Option<Params>> {
        Ok(match &self.cfg {
            Some(path_str) if path_str == "none" => None,
            Some(path) => {
                let path = PathBuf::from(path);

                eprintln!("📄 Using custom params file: {}", path.display());

                Some(Params::from_file(path)?)
            }
            None => {
                let default_cfg_path = PathBuf::from(Self::DEFAULT_PARAMS_PATH);

                if default_cfg_path.exists() {
                    eprintln!(
                        "📄 Using default params file: {}",
                        default_cfg_path.display()
                    );

                    Some(Params::from_file(default_cfg_path)?)
                } else {
                    None
                }
            }
        })
    }
}

/// Initializes structured logging for command execution.
///
/// The caller provides the default level, while environment overrides from `RUST_LOG`
/// still participate via [`EnvFilter::from_env_lossy`]. Verbose Cranelift logs are
/// disabled unconditionally because they are too noisy for normal CLI use.
fn enable_logging(logging_level_name: &str) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(logging_level_name.parse()?)
                .from_env_lossy()
                .add_directive("wasmtime_cranelift=off".parse()?)
                .add_directive("cranelift=off".parse()?),
        )
        .try_init()
        .map_err(|e| anyhow::anyhow!("failed to initialize logger: {e}"))
}
