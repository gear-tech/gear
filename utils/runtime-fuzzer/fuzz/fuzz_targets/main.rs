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

use chrono::NaiveDateTime;
use libfuzzer_sys::fuzz_target;
use once_cell::sync::OnceCell;
use std::{
    fs::{self, OpenOptions},
    io::Write,
};

const SEEDS_STORE_DIR: &str = "fuzzing-seeds-dir";
const SEEDS_STORE_FILE: &str = "fuzzing-seeds";

static RUN_DIR: OnceCell<String> = OnceCell::new();

fuzz_target!(|seed: u64| {
    gear_utils::init_default_logger();

    dump_seed(seed).expect("internal error: failed dumping seed");

    log::info!("Running the seed {seed}");
    runtime_fuzzer::run(seed);
});

// Dumps seed to the `SEEDS_STORE_FILE` file inside `SEEDS_STORE_DIR`
// directory before running fuzz test.
fn dump_seed(seed: u64) -> Result<(), String> {
    let fuzzing_seeds_dir = RUN_DIR.get_or_init(|| {
        let date_time = NaiveDateTime::from_timestamp_millis(gear_utils::now_millis() as i64)
            .expect("timestamp is in i64 range");
        let fuzzing_seeds_dir = format!(
            "{SEEDS_STORE_DIR}-{}",
            date_time.format("%Y-%m-%dT%H:%M:%S")
        );
        fs::create_dir_all(&fuzzing_seeds_dir)
            .map(|_| fuzzing_seeds_dir)
            .expect("internal error: can't create file")
    });
    let fuzzing_seeds_file = format!("{fuzzing_seeds_dir}/{SEEDS_STORE_FILE}");

    OpenOptions::new()
        .create(true)
        .append(true)
        .open(fuzzing_seeds_file)
        .and_then(|mut file| writeln!(file, "{seed}"))
        .map_err(|e| e.to_string())
}
