// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lazy page fuzzer", version, about = "lazy pages fuzzer")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run fuzzer normally
    Run(RunArgs),

    /// Reproduce fuzzer run with a specific instance seed
    Reproduce {
        /// 64-char hex string representing [u8; 32]
        instance_seed: String,
    },
    /// DO NOT USE, intended for internal use only, not a public command
    Worker {
        // Token to identify the worker
        #[arg(long)]
        token: String,
        // Worker time to live in seconds (after which it will exit)
        #[arg(long)]
        ttl: u64,
        // CPU core affinity for the worker
        #[arg(long)]
        cpu_affinity: usize,
    },
}

#[derive(Args)]
pub struct RunArgs {
    /// Duration in seconds for which the fuzzer will run
    #[arg(long)]
    pub duration_seconds: Option<u64>,
}
