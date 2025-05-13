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

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use arbitrary::{Arbitrary as _, Unstructured};
use clap::Parser;
use cli::{Cli, Commands, RunArgs};
use lazy_pages_fuzzer::GeneratedModule;
use seeds::{derivate_seed, generate_instance_seed, generate_seed};
use uitls::{hex_to_string, string_to_hex};

mod cli;
mod seeds;
mod uitls;

const SEED_PATH: &str = "seed.bin";
const STATS_PRINT_INTERVAL: u64 = 30;

#[derive(Default)]
struct Stats {
    instances: u64,
}

fn init_logger() {
    env_logger::init();
}

fn ts() -> u64 {
    // Get the current timestamp in milliseconds
    let now = std::time::SystemTime::now();
    now.duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}

fn generate_seed_if_not_exists() -> Vec<u8> {
    // Check if the seed file exists
    if !std::path::Path::new(SEED_PATH).exists() {
        // Generate a random seed with 350000 bytes length
        let input_seed = generate_seed(ts());
        // write the seed to a file
        std::fs::write(SEED_PATH, &input_seed).expect("Failed to write seed to file");
        log::info!("Input seed written to seed.bin");
        input_seed
    } else {
        // If the seed file exists, read it
        let seed = std::fs::read(SEED_PATH).expect("Failed to read seed from file");
        log::info!("Seed file already exists, skipping seed creation.");
        seed
    }
}

fn main() {
    init_logger();

    let cli: Cli = Cli::parse();

    match cli.command {
        Commands::Run(RunArgs {
            print_module_and_exit,
        }) => {
            run(print_module_and_exit);
        }
        Commands::Reproduce { instance_seed } => {
            let instance_seed = string_to_hex(&instance_seed);
            reproduce(instance_seed);
        }
    }
}

fn run(print_module_and_exit: bool) {
    log::info!("Starting lazy pages fuzzer");

    let input_seed = generate_seed_if_not_exists();
    let mut status = Stats::default();
    let mut stats_ts = Instant::now();

    loop {
        let instance_seed = generate_instance_seed(ts());

        let derived_seed = derivate_seed(&input_seed, &instance_seed);

        let mut u = Unstructured::new(&derived_seed);
        let m = GeneratedModule::arbitrary(&mut u).expect("Failed to generate module");
        // Instrument the module
        let m = m.enhance().unwrap();

        if print_module_and_exit {
            log::info!("Generated module: {m:#?}");
            return;
        }

        let defuse = AtomicBool::new(false);
        let _guard = scopeguard::guard(&defuse, |defuse| {
            if !defuse.load(Ordering::SeqCst) {
                log::error!("*****Instance seed: {}", hex_to_string(&instance_seed));
            }
        });

        match lazy_pages_fuzzer::run(m) {
            Err(_) => panic!("failed to fuzz"),
            Ok(_) => (),
        }

        status.instances += 1;
        let elapsed_sec = stats_ts.elapsed().as_secs();

        if elapsed_sec > STATS_PRINT_INTERVAL {
            log::info!("Fuzzed {} instances/s", status.instances / elapsed_sec);
            status.instances = 0;
            stats_ts = Instant::now();
        }

        defuse.store(true, Ordering::SeqCst);
    }
}

fn reproduce(instance_seed: [u8; 32]) {
    log::info!(
        "Reproducing fuzzer run with instance seed: {}",
        hex_to_string(&instance_seed)
    );

    let input_seed = generate_seed_if_not_exists();
    let derived_seed = derivate_seed(&input_seed, &instance_seed);

    let mut u = Unstructured::new(&derived_seed);
    let m = GeneratedModule::arbitrary(&mut u).expect("Failed to generate module");
    // Instrument the module
    let m = m.enhance().unwrap();
    log::info!("Generated module: {m:#?}");

    match lazy_pages_fuzzer::run(m) {
        Err(_) => panic!("failed to fuzz"),
        Ok(_) => (),
    }

    log::info!("Reproduced successfully");
}
