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
use clap::{CommandFactory, Parser};
use tracing_subscriber::filter::EnvFilter;

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

impl App {
    fn run(&self) -> Result<()> {
        let artifact = self.command.collect()?;
        tracing::info!("The artifact has been generated at {:?}", artifact.root);
        Ok(())
    }
}

fn main() -> Result<()> {
    let app = App::parse();

    // Replace the binary name to library name.
    let name = App::command().get_name().to_string().replace('-', "_");
    let env = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(match app.verbose {
        0 => format!("{name}=info"),
        1 => format!("{name}=debug"),
        2 => "debug".into(),
        _ => "trace".into(),
    }));

    tracing_subscriber::fmt().with_env_filter(env).init();
    app.run()
}
