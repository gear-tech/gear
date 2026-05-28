// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
