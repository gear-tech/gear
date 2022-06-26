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

use clap::{Arg, Command};
use log::debug;
use pwasm_utils::{
    self as utils,
    parity_wasm::{self, elements::Module},
};
use std::{fs, path::PathBuf};
use wasm_proc::Optimizer;

#[derive(Debug)]
enum CliError {
    UndefinedPaths,
    InvalidSkip,
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UndefinedPaths => write!(f, "Paths to .wasm files are undefined"),
            Self::InvalidSkip => write!(f, "Multiple skipping functional"),
        }
    }
}

impl std::error::Error for CliError {}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Arg::new("path")
        .short('p')
        .long("path")
        .required(true)
        .takes_value(true)
        .multiple_values(true)
        .help("Specifies path to .wasm file(-s)");

    let skip_meta = Arg::new("skip-meta")
        .long("skip-meta")
        .takes_value(false)
        .help("Skips metadata optimization");

    let skip_opt = Arg::new("skip-opt")
        .long("skip-opt")
        .takes_value(false)
        .help("Skips chain optimization");

    let verbose = Arg::new("verbose")
        .short('v')
        .long("verbose")
        .takes_value(false)
        .help("Provides debug logging info");

    let skip_stack_end = Arg::new("skip-stack-end")
        .long("skip-stack-end")
        .takes_value(false)
        .help("Skips creating of global export with stack end addr");

    let app = Command::new("wasm-proc").args(&[path, skip_meta, skip_opt, skip_stack_end, verbose]);

    let matches = app.get_matches();

    if matches.contains_id("verbose") {
        env_logger::Builder::from_env(env_logger::Env::new().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_default_env();
    }

    let wasm_files: Vec<String> = matches
        .get_many("path")
        .ok_or(CliError::UndefinedPaths)?
        .cloned()
        .collect();

    let skip_stack_end = matches.contains_id("skip-stack-end");
    let skip_meta = matches.contains_id("skip-meta");
    let skip_opt = matches.contains_id("skip-opt");

    if skip_meta && skip_opt {
        return Err(Box::new(CliError::InvalidSkip));
    }

    for file in &wasm_files {
        if !file.ends_with(".wasm") || file.ends_with(".meta.wasm") || file.ends_with(".opt.wasm") {
            continue;
        }

        let file = PathBuf::from(file);
        let res = gear_wasm_builder::optimize::optimize_wasm(file.clone(), "s", true)?;

        log::info!(
            "wasm-opt: {} {} Kb -> {} Kb",
            res.dest_wasm.display(),
            res.original_size,
            res.optimized_size
        );

        let mut optimizer = Optimizer::new(file)?;

        if !skip_stack_end {
            optimizer.insert_stack_and_export();
        }

        if !skip_opt {
            let code = optimizer.optimize()?;
            let path = optimizer.optimized_file_name();
            fs::write(path, code)?;
        }

        if !skip_meta {
            let code = optimizer.metadata()?;
            let path = optimizer.metadata_file_name();
            fs::write(path, code)?;
        }
    }

    Ok(())
}
