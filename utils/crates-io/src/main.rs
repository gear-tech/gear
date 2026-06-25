// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Utility for managing Gear packages on crates.io.
//!
//! Management includes:
//! - workspace patching (`build` sub-command)
//! - publishing packages (`publish` sub-command)
//!   - simulate publishing to crates.io using local registry (`--simulate` option)
//!   - real publishing to crates.io
//!
//! If you are looking for examples of how to run it, take a look at `.github/workflows/crates-io.yml`.
//!
//! WARNING: Before running, please ensure you use `--simulate`.
//! Otherwise, this could result in packages being published on your behalf!

use anyhow::Result;
use clap::Parser;
use crates_io::Publisher;
use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

/// The command to run.
#[derive(Clone, Debug, Parser)]
enum Command {
    /// Build manifests for packages that to be published.
    Build {
        /// The version to publish.
        #[clap(long, short)]
        version: Option<String>,
    },
    /// Publish packages.
    Publish {
        /// The version to publish.
        #[clap(long, short)]
        version: Option<String>,
        /// Simulates publishing of packages.
        #[clap(long, short)]
        simulate: bool,
        /// Path to registry for simulation.
        #[arg(short, long)]
        registry_path: Option<PathBuf>,
    },
}

/// Gear crates-io manager command line interface
///
/// NOTE: this binary should not be used locally
/// but run in CI.
#[derive(Debug, Parser)]
pub struct Opt {
    #[clap(subcommand)]
    command: Command,
}

#[tokio::main]
async fn main() -> Result<()> {
    let Opt { command } = Opt::parse();

    match command {
        Command::Publish {
            version,
            simulate,
            registry_path,
        } => {
            let mut publisher = Publisher::with_simulation(simulate, registry_path)?
                .build(true, version)
                .await?;
            publisher.prepare_publish()?;
            // TODO: return this check back
            // publisher.check()?;
            let result = publisher.publish();
            if result.is_ok() || ask_restore_after_error()? {
                publisher.restore()?;
            }
            result
        }
        Command::Build { version } => {
            let mut publisher = Publisher::new()?.build(false, version).await?;
            publisher.prepare_publish()?;
            Ok(())
        }
    }
}

fn ask_restore_after_error() -> Result<bool> {
    if !io::stdin().is_terminal() {
        eprintln!("Publishing failed in non-interactive mode; restoring local changes.");
        return Ok(true);
    }

    loop {
        print!("Publishing failed. Restore local changes? [y/n]: ");
        io::stdout().flush()?;

        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;

        match answer.trim().to_ascii_lowercase().as_str() {
            "y" => return Ok(true),
            "n" => return Ok(false),
            _ => eprintln!("Please answer `y` or `n`."),
        }
    }
}
