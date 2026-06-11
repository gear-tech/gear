// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

fn main() {
    // force subxt proc macro to use updated metadata
    println!("cargo:rerun-if-changed=vara_runtime.scale");

    let metadata = std::fs::read("vara_runtime.scale").expect("failed to read Vara metadata");
    let fingerprint = metadata.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    println!("cargo:rustc-env=GSDK_METADATA_FINGERPRINT={fingerprint:016x}");
}
