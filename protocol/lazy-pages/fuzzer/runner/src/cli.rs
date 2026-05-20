// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
