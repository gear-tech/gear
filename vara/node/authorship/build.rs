// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use std::{env, fs, path::PathBuf};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_dir = PathBuf::from(out_dir);
    // create placeholder in `OUT_DIR`
    // so `env!("OUT_DIR")` can be used for executor module caching
    fs::write(out_dir.join("placeholder"), "placeholder file").unwrap();
}
