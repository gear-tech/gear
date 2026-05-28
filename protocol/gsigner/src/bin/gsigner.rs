// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use anyhow::Result;
use clap::Parser;
use gsigner::cli::{GSignerCli, display_result_with_format, execute_command};

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = GSignerCli::parse();
    let result = execute_command(cli.command)?;
    display_result_with_format(&result, cli.format);

    Ok(())
}
