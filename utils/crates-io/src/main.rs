// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! mini-program for publishing packages to crates.io.

use anyhow::Result;
use clap::Parser;
use crates_io::Publisher;
use std::path::PathBuf;

/// The command to run.
#[derive(Clone, Debug, Parser)]
enum Command {
    /// Build manifests for packages that to be published.
    Build,
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
            let publisher = Publisher::with_simulation(simulate, registry_path)?
                .build(true, version)
                .await?
                .check()?;
            let result = publisher.publish();
            publisher.restore()?;
            result
        }
        Command::Build => {
            Publisher::new()?.build(false, None).await?;
            Ok(())
        }
    }
}
