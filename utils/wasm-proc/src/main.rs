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
use std::path::PathBuf;

#[derive(Debug)]
enum Error {
    OptimizerFailed,
    SerializationFailed(parity_wasm::elements::Error),
    UndefinedPaths,
    InvalidSkip,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OptimizerFailed => write!(f, "Optimizer failed"),
            Self::SerializationFailed(e) => write!(f, "Serialization failed {}", e),
            Self::UndefinedPaths => write!(f, "Paths to .wasm files are undefined"),
            Self::InvalidSkip => write!(f, "Multiple skipping functional"),
        }
    }
}

impl std::error::Error for Error {}

/// Calls chain optimizer
fn optimize(path: &str, mut binary_module: Module) -> Result<(), Box<dyn std::error::Error>> {
    debug!("*** Processing chain optimization: {}", path);

    let binary_file_name = PathBuf::from(path).with_extension("opt.wasm");

    utils::optimize(
        &mut binary_module,
        vec!["handle", "handle_reply", "init", "__gear_stack_end"],
    )
    .map_err(|_| Error::OptimizerFailed)?;

    gear_wasm_builder::optimize::check_exports(&binary_module, &binary_file_name)?;

    parity_wasm::serialize_to_file(binary_file_name.clone(), binary_module)
        .map_err(Error::SerializationFailed)?;

    debug!("Optimized wasm: {}", binary_file_name.to_string_lossy());
    Ok(())
}

/// Calls metadata optimizer
fn optimize_meta(
    path: &str,
    mut metadata_module: Module,
) -> Result<(), Box<dyn std::error::Error>> {
    debug!("*** Processing metadata optimization: {}", path);

    let metadata_file_name = PathBuf::from(path).with_extension("meta.wasm");

    utils::optimize(
        &mut metadata_module,
        vec![
            "meta_init_input",
            "meta_init_output",
            "meta_async_init_input",
            "meta_async_init_output",
            "meta_handle_input",
            "meta_handle_output",
            "meta_async_handle_input",
            "meta_async_handle_output",
            "meta_registry",
            "meta_title",
            "meta_state",
            "meta_state_input",
            "meta_state_output",
        ],
    )
    .map_err(|_| Error::OptimizerFailed)?;

    parity_wasm::serialize_to_file(metadata_file_name.clone(), metadata_module)
        .map_err(Error::SerializationFailed)?;

    debug!("Metadata wasm: {}", metadata_file_name.to_string_lossy());
    Ok(())
}

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
        .ok_or(Error::UndefinedPaths)?
        .cloned()
        .collect();

    let skip_stack_end = matches.contains_id("skip-stack-end");
    let skip_meta = matches.contains_id("skip-meta");
    let skip_opt = matches.contains_id("skip-opt");

    if skip_meta && skip_opt {
        return Err(Box::new(Error::InvalidSkip));
    }

    for file in &wasm_files {
        if !file.ends_with(".wasm") || file.ends_with(".meta.wasm") || file.ends_with(".opt.wasm") {
            continue;
        }

        let res = gear_wasm_builder::optimize::optimize_wasm(PathBuf::from(file), "s", true)?;

        log::info!(
            "wasm-opt: {} {} Kb -> {} Kb",
            res.dest_wasm.display(),
            res.original_size,
            res.optimized_size
        );

        let mut module = parity_wasm::deserialize_file(file)?;

        if !skip_stack_end {
            let _ = gear_wasm_builder::insert_stack_end_export(&mut module)
                .map_err(|s| log::debug!("{}", s));
        }

        if !skip_opt {
            optimize(file, module.clone())?;
        }
        if !skip_meta {
            optimize_meta(file, module.clone())?;
        }
    }

    Ok(())
}
