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

//! Script to reproduce crashes found by `runtime-fuzzer`.
//!
//! This file is a temporary solution until #2313 is implemented.
//! Fuzzer dumps all the seed into the file, so the full run can
//! be reproduced in case of the fail.
//!
//! Just simply run `cargo run -- -p <path_to_fuzz_seeds>`.

use anyhow::Result;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Params {
    /// Path to the file, which contains seeds from previously run fuzzer.
    #[structopt(short = "p", long, parse(from_os_str))]
    pub path: PathBuf,
}

fn main() -> Result<()> {
    gear_utils::init_default_logger();

    let file_reader = create_file_reader(Params::from_args().path)?;

    // Read seeds and run test against all of them.
    for line in file_reader.lines() {
        let seed: u64 = line?.trim().parse()?;

        log::info!("Reproducing run with the seed - {seed}");

        runtime_fuzzer::run(seed);
    }

    Ok(())
}

fn create_file_reader(path: PathBuf) -> Result<BufReader<File>> {
    let file = File::open(path)?;

    Ok(BufReader::new(file))
}
