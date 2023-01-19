// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

#[allow(unused)]
pub(crate) const HACK: u64 = 101010101;

mod command;
pub(crate) mod util;

use std::{path::PathBuf, str::FromStr};

#[derive(Clone, Debug)]
pub enum Runtime {
    Gear,
    Vara,
}

impl FromStr for Runtime {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "gear" => Ok(Runtime::Gear),
            "vara" => Ok(Runtime::Vara),
            _ => Err("Unknown runtime"),
        }
    }
}

/// The `runtests` command used to test gear with yaml.
#[derive(Debug, clap::Parser)]
pub struct RuntimeTestCmd {
    /// Input dir/file with yaml for testing.
    #[arg(value_parser)]
    pub input: Vec<PathBuf>,

    /// Produce output in the (almost) JUnit/XUnit XML format.
    #[arg(long, value_parser)]
    pub generate_junit: Option<PathBuf>,

    #[arg(long, value_parser)]
    pub runtime: Runtime,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub shared_params: sc_cli::SharedParams,
}

impl RuntimeTestCmd {
    pub fn run(&self, _cfg: sc_service::Configuration) -> sc_cli::Result<()> {
        match self.runtime {
            #[cfg(feature = "gear-native")]
            Runtime::Gear => self.run_gear(),
            #[cfg(not(feature = "gear-native"))]
            Runtime::Gear => {
                Err(String::from("CLI command built without `gear-native` feature").into())
            }
            #[cfg(feature = "vara-native")]
            Runtime::Vara => self.run_vara(),
            #[cfg(not(feature = "vara-native"))]
            Runtime::Vara => {
                Err(String::from("CLI command built without `gear-native` feature").into())
            }
        }
    }

    #[cfg(feature = "gear-native")]
    fn run_gear(&self) -> sc_cli::Result<()> {
        command::gear::run(self)
    }

    #[cfg(feature = "vara-native")]
    fn run_vara(&self) -> sc_cli::Result<()> {
        command::vara::run(self)
    }
}
