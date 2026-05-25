// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use anyhow::Result;
use clap::Parser;
use ethexe_cli::Cli;

/// Parses the CLI and delegates execution to [`ethexe_cli::Cli`].
fn main() -> Result<()> {
    let cli = Cli::parse();

    cli.run()
}
