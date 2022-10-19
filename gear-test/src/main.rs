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

mod address;
mod check;
mod js;
mod manager;
mod proc;
mod sample;

use clap::Parser;
use gear_backend_wasmi::WasmiEnvironment;
use manager::InMemoryExtManager;

#[derive(Parser)]
struct Opts {
    /// Skip messages checks
    #[clap(long, value_parser)]
    pub skip_messages: bool,
    /// Skip allocations checks
    #[clap(long, value_parser)]
    pub skip_allocations: bool,
    /// Skip memory checks
    #[clap(long, value_parser)]
    pub skip_memory: bool,
    /// JSON sample file(s) or dir
    #[clap(value_parser)]
    pub input: Vec<std::path::PathBuf>,
    /// A level of verbosity
    #[clap(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

pub fn main() -> anyhow::Result<()> {
    let opts: Opts = Opts::parse();
    let print_logs = !matches!(opts.verbose, 0);
    check::check_main::<InMemoryExtManager, WasmiEnvironment<_>, _>(
        opts.input.to_vec(),
        opts.skip_messages,
        opts.skip_allocations,
        opts.skip_memory,
        print_logs,
        InMemoryExtManager::default,
    )
}
