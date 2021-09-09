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

use clap::{App, Arg};
use log::debug;
use pwasm_utils::{
    self as utils,
    parity_wasm::{self, elements::Module},
};
use std::path::PathBuf;

/// Calls chain optimizer
fn optimize(path: &str, mut binary_module: Module) {
    debug!("\n*** Processing chain optimization: {}", path);

    let binary_file_name = PathBuf::from(path).with_extension("opt.wasm");

    if let Err(_) = utils::optimize(&mut binary_module, vec!["handle", "init"]) {
        debug!("Optimizer failed");
    }

    if let Err(e) = parity_wasm::serialize_to_file(binary_file_name.clone(), binary_module) {
        debug!("Serialization failed: {}", e);
    }

    debug!("Optimized wasm: {}", binary_file_name.to_string_lossy());
}

/// Calls metadata optimizer
fn optimize_meta(path: &str, mut metadata_module: Module) {
    debug!("\n*** Processing metadata optimization: {}", path);

    let metadata_file_name = PathBuf::from(path).with_extension("meta.wasm");

    if let Err(_) = utils::optimize(
        &mut metadata_module,
        vec![
            "meta_init_input",
            "meta_init_output",
            "meta_input",
            "meta_output",
            "meta_title",
            "meta_types",
        ],
    ) {
        debug!("Optimizer failed");
    }

    if let Err(e) = parity_wasm::serialize_to_file(metadata_file_name.clone(), metadata_module) {
        debug!("Serialization failed: {}", e);
    }

    debug!("Metadata wasm: {}", metadata_file_name.to_string_lossy());
}

fn main() {
    let meta = Arg::new("meta")
        .short('m')
        .long("meta")
        .takes_value(false)
        .about("Provides a metadata .meta.wasm file");

    let opt = Arg::new("optimize")
        .short('o')
        .long("optimize")
        .takes_value(false)
        .about("Provides an optimized .opt.wasm file");

    let path = Arg::new("path")
        .short('p')
        .long("path")
        .required(true)
        .index(1)
        .takes_value(true)
        .multiple_values(true)
        .about("Specifies path to .wasm file(-s)");

    let verbose = Arg::new("verbose")
        .short('v')
        .long("verbose")
        .takes_value(false)
        .about("Set environment variable RUST_LOG=debug");

    let app = App::new("wasm-proc").args(&[meta, opt, path, verbose]);

    let matches = app.get_matches();

    if matches.is_present("verbose") {
        env_logger::Builder::from_env(env_logger::Env::new().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_default_env();
    }

    let wasm_files: Vec<&str> = matches
        .values_of("path")
        .expect("Path to wasm files is required")
        .collect();

    let o = matches.is_present("optimize");
    let m = matches.is_present("meta");

    for file in wasm_files {
        if let Ok(module) = parity_wasm::deserialize_file(file) {
            if o || !(o || m) {
                optimize(file, module.clone());
            }
            if m || !(o || m) {
                optimize_meta(file, module.clone());
            }
        } else {
            debug!("Failed to load wasm file: {}", file);
        }
    }
}
