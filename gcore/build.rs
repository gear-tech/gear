// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::env;

#[cfg(not(feature = "compile-shim"))]
fn main() {
    if env::var("TARGET").unwrap() == "wasm32v1-none" {
        println!("cargo:rustc-link-lib=static=gcore_shim");
        println!(
            "cargo:rustc-link-search=native={}",
            env::var("CARGO_MANIFEST_DIR").unwrap()
        );
    }
}

#[cfg(feature = "compile-shim")]
fn main() {
    if env::var("TARGET").unwrap() != "wasm32v1-none" {
        return;
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/shim.c");

    let mut builder = cc::Build::new();

    if option_env!("CC") == Some("clang") {
        builder.flag("-flto");
    }

    builder
        .file("src/shim.c")
        .opt_level(2)
        .compile("gcore_shim");
}
