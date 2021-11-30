// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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
mod runner;
mod sample;

use clap::Parser;
use gear_core::storage::InMemoryStorage;

#[derive(Parser)]
struct Opts {
    /// Skip messages checks
    #[clap(long)]
    pub skip_messages: bool,
    /// Skip allocations checks
    #[clap(long)]
    pub skip_allocations: bool,
    /// Skip memory checks
    #[clap(long)]
    pub skip_memory: bool,
    /// JSON sample file(s) or dir
    pub input: Vec<std::path::PathBuf>,
    /// A level of verbosity
    #[clap(short, long, parse(from_occurrences))]
    verbose: i32,
}

pub fn main() -> anyhow::Result<()> {
    let opts: Opts = Opts::parse();
    let print_logs = !matches!(opts.verbose, 0);
    check::check_main::<InMemoryStorage, _>(
        opts.input.to_vec(),
        opts.skip_messages,
        opts.skip_allocations,
        opts.skip_memory,
        print_logs,
        InMemoryStorage::default,
        None,
    )
}
