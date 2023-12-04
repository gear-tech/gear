// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! mini-program for publishing packages to crates.io.

use anyhow::Result;
use clap::Parser;
use crates_io_manager::Publisher;

/// The command to run.
#[derive(Clone, Debug, Parser)]
enum Command {
    /// Check packages that to be published.
    Check,
    /// Publish packages.
    Publish,
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

fn main() -> Result<()> {
    let Opt { command } = Opt::parse();

    let publisher = Publisher::new()?.build()?;
    match command {
        Command::Check => publisher.check(),
        Command::Publish => publisher.publish(),
    }
}
