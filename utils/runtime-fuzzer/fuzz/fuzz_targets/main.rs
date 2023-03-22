// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

#![no_main]

use libfuzzer_sys::fuzz_target;
use once_cell::sync::OnceCell;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

const SEEDS_STORE_DIR: &str = "fuzzing-seeds-dir";
const SEEDS_STORE_FILE: &str = "fuzzing-seeds";

static RUN_INTIALIZED: OnceCell<String> = OnceCell::new();

fuzz_target!(|seed: u64| {
    gear_utils::init_default_logger();

    dump_seed(seed).expect("internal error: failed dumping seed");

    log::info!("Running the seed {seed}");
    runtime_fuzzer::run(seed);
});

// Dumps seed to the `SEEDS_STORE_FILE` file inside `SEEDS_STORE_DIR`
// directory before running fuzz test.
//
// If directory already exists for the current run, it will be cleared.
fn dump_seed(seed: u64) -> Result<(), String> {
    let seeds_file = RUN_INTIALIZED.get_or_try_init(|| {
        let seeds_dir = Path::new(SEEDS_STORE_DIR);
        if seeds_dir.exists() {
            fs::remove_dir_all(seeds_dir).map_err(|e| e.to_string())?;
        }
        fs::create_dir_all(seeds_dir).map_err(|e| e.to_string())?;

        Ok::<_, String>(format!("{SEEDS_STORE_DIR}/{SEEDS_STORE_FILE}"))
    })?;

    OpenOptions::new()
        .create(true)
        .append(true)
        .open(seeds_file)
        .and_then(|mut file| writeln!(file, "{seed}"))
        .map_err(|e| e.to_string())
}
