// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! This crate provides the main CLI interface.

use crate::{
    app::{App, Opts},
    cmd::Command,
};
use anyhow::Result;
use clap::Parser;

/// Interact with Gear API via node RPC.
#[derive(Debug, Clone, Parser)]
#[clap(author, version)]
pub struct Cli {
    #[command(flatten)]
    opts: Opts,

    /// Command to run.
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        App::new(self.opts).run(self.command).await
    }

    pub fn run_blocking(self) -> Result<()> {
        tokio::runtime::Runtime::new()?.block_on(self.run())
    }
}
