// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

fn main() {
    // force subxt proc macro to use updated metadata
    println!("cargo:rerun-if-changed=vara_runtime.scale");
}
