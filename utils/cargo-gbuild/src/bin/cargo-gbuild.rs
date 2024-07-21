// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use anyhow::Result;
use cargo_gbuild::GBuild;
use clap::Parser;
use std::env;
use tracing_subscriber::filter::EnvFilter;

const CUSTOM_COMMAND_NAME: &str = "gbuild";

/// Command `gbuild` as cargo extension.
#[derive(Parser)]
#[clap(author, version)]
#[command(name = "cargo-gbuild")]
struct App {
    /// The verbosity level
    #[clap(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// The gbuild command.
    #[clap(flatten)]
    pub command: GBuild,
}

fn main() -> Result<()> {
    let args = env::args().enumerate().filter_map(|(idx, arg)| {
        if idx == 1 && arg == CUSTOM_COMMAND_NAME {
            return None;
        }

        Some(arg)
    });

    let app = App::parse_from(args);

    // Replace the binary name to library name.
    let name = env!("CARGO_PKG_NAME").replace('-', "_");
    let env = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(match app.verbose {
        0 => format!("{name}=info"),
        1 => format!("{name}=debug"),
        2 => format!("{name}=trace"),
        _ => "trace".into(),
    }));

    tracing_subscriber::fmt()
        .with_env_filter(env)
        .without_time()
        .with_target(false)
        .init();
    app.command.run().map(|_| ())
}
