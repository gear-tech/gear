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

use std::{process, thread, time::Instant};

use arbitrary::{Arbitrary as _, Unstructured};
use clap::Parser;
use cli::{Cli, Commands};
use lazy_pages_fuzzer::GeneratedModule;
use seeds::{derivate_seed, generate_seed};
use utils::{cast_slice, hex_to_string, string_to_hex};

use crate::cli::RunArgs;

mod cli;
mod seeds;
mod utils;
mod worker;

const SEED_SIZE_IN_U32: usize = 8192;
const SEED_PATH: &str = "seed.bin";
const STATS_PRINT_INTERVAL: u64 = 10;
const WORKER_TTL_SEC: u64 = 10;

#[derive(Default)]
struct FuzzerStats {
    instances_fuzzed: u64,
}

fn init_logger() {
    env_logger::init();
}

fn ts() -> u64 {
    // Get the current timestamp in milliseconds
    let now = std::time::SystemTime::now();
    now.duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_micros() as u64
}

fn generate_or_read_seed(silent: bool) -> Vec<u32> {
    // Check if the seed file exists
    if std::path::Path::new(SEED_PATH).exists() {
        // If the seed file exists, read it
        let seed = std::fs::read(SEED_PATH).expect("Failed to read seed from file");
        if !silent {
            log::info!("Seed file already exists, skipping seed creation.");
        }
        seed.chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("Invalid seed chunk size")))
            .collect()
    } else {
        let input_seed = generate_seed(ts());
        // write the seed to a file
        std::fs::write(SEED_PATH, cast_slice(&input_seed)).expect("Failed to write seed to file");
        if !silent {
            log::info!("Input seed written to seed.bin");
        }
        input_seed
    }
}

fn main() {
    init_logger();

    let cli: Cli = Cli::parse();

    match cli.command {
        Commands::Run(RunArgs { duration_seconds }) => {
            run_fuzzer(duration_seconds);
        }
        Commands::Reproduce { instance_seed } => {
            let instance_seed = string_to_hex(&instance_seed);
            reproduce(instance_seed);
        }
        Commands::Worker {
            token,
            ttl,
            cpu_affinity,
        } => {
            worker::run(token, ttl, cpu_affinity);
        }
    }
}

fn run_fuzzer(duration_seconds: Option<u64>) {
    log::info!("Starting lazy pages fuzzer");

    if let Some(duration_seconds) = duration_seconds {
        log::info!("Fuzzer will run for {duration_seconds} seconds");
    }

    let _ = generate_or_read_seed(false);
    let mut status = FuzzerStats::default();
    let start_ts = Instant::now();
    let mut stats_ts = Instant::now();

    let mut workers = worker::Workers::spawn(
        WORKER_TTL_SEC,
        thread::available_parallelism().unwrap().into(),
    );

    let report = workers.run(|| {
        status.instances_fuzzed += 1;
        let elapsed_sec = stats_ts.elapsed().as_secs();

        if elapsed_sec > STATS_PRINT_INTERVAL {
            log::info!(
                "Fuzzed {} instances/s",
                status.instances_fuzzed / elapsed_sec
            );
            status.instances_fuzzed = 0;
            stats_ts = Instant::now();
        }

        if let Some(duration_seconds) = duration_seconds
            && start_ts.elapsed().as_secs() >= duration_seconds
        {
            log::info!("Fuzzer run completed after {duration_seconds} seconds");
            process::exit(0);
        }
    });

    if let Some(report) = report {
        eprintln!("{report:#?}");
        process::exit(report.exit_code);
    }
}

fn reproduce(instance_seed: [u8; 32]) {
    log::info!(
        "Reproducing fuzzer run with instance seed: {}",
        hex_to_string(&instance_seed)
    );

    let input_seed = generate_or_read_seed(false);
    let derived_seed = derivate_seed(&input_seed, &instance_seed);

    let mut u = Unstructured::new(&derived_seed);
    let m = GeneratedModule::arbitrary(&mut u).expect("Failed to generate module");
    // Instrument the module
    let m = m.enhance().unwrap();

    if lazy_pages_fuzzer::run(m).is_err() {
        panic!("failed to fuzz")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn test_generate_or_read_seed() {
        // Remove the seed file if it exists to start fresh
        if Path::new(SEED_PATH).exists() {
            std::fs::remove_file(SEED_PATH).expect("Failed to remove seed file");
        }

        // Test that the seed is generated and written to a file
        let seed = generate_or_read_seed(true);
        assert!(!seed.is_empty());
        assert!(std::path::Path::new(SEED_PATH).exists());

        // Test that the seed is read from the file if it exists
        let seed2 = generate_or_read_seed(true);
        assert_eq!(seed, seed2);
    }
}
