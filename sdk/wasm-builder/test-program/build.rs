// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use std::fs;

fn main() {
    gear_wasm_builder::build();

    // to be built by other tests
    fs::write("src/rebuild_test.rs", "").unwrap();
}
