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

pub(crate) const HACK: u64 = 101010101;

use std::path::PathBuf;

mod command;
mod util;

/// The `runtests` command used to test gear with yaml.
#[derive(Debug, clap::Parser)]
pub struct GearRuntimeTestCmd {
    /// Input dir/file with yaml for testing.
    #[clap(value_parser)]
    pub input: Vec<PathBuf>,

    /// Produce output in the (almost) JUnit/XUnit XML format.
    #[clap(long, value_parser)]
    pub generate_junit: Option<PathBuf>,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub shared_params: sc_cli::SharedParams,
}
