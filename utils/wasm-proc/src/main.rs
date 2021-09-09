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
use pwasm_utils::{self as utils, parity_wasm};
use std::path::PathBuf;

fn main() {
    let matches = App::new("wasm-proc")
        .arg(
            Arg::new("input")
                .index(1)
                .required(true)
                .about("Input WASM file"),
        )
        .get_matches();

    let input = matches
        .value_of("input")
        .expect("Input paramter is required by clap above; qed");

    let module = parity_wasm::deserialize_file(&input).expect("Failed to load wasm file");

    // Invoke optimizer for the chain
    let mut binary_module = module.clone();
    let binary_file_name = PathBuf::from(input).with_extension("opt.wasm");
    utils::optimize(&mut binary_module, vec!["handle", "init"]).expect("Optimizer failed");
    parity_wasm::serialize_to_file(binary_file_name.clone(), binary_module)
        .expect("Serialization failed");

    println!("Optimized wasm: {}", binary_file_name.to_string_lossy());

    // Invoke optimizer for the metadata
    let mut metadata_module = module.clone();
    let metadata_file_name = PathBuf::from(input).with_extension("meta.wasm");
    utils::optimize(
        &mut metadata_module,
        vec![
            "meta_input",
            "meta_output",
            "meta_init_input",
            "meta_init_output",
            "meta_title",
        ],
    )
    .expect("Metadata optimizer failed");
    parity_wasm::serialize_to_file(metadata_file_name.clone(), metadata_module)
        .expect("Serialization failed");

    println!("Metadata wasm: {}", metadata_file_name.to_string_lossy());
}
