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
use ccli::{clap, App, Parser};

/// Command `gbuild` as cargo extension.
#[derive(Parser)]
#[clap(author, version)]
#[command(name = "cargo_gbuild")]
struct Opt {
    /// The verbosity level
    #[clap(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// The gbuild command.
    #[clap(flatten)]
    pub command: GBuild,
}

impl App for Opt {
    fn verbose(&self) -> u8 {
        self.verbose
    }

    fn run(&self) -> Result<()> {
        self.command.build()
    }
}

fn main() {
    Opt::start().expect("Failed to process cargo-gbuild.");
}
