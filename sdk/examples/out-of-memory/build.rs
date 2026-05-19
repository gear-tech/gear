// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gear_wasm_builder::WasmBuilder;

fn main() {
    // We are forcing recommended nightly toolchain due to the need to compile this
    // program with `oom-handler` feature. The WASM binary of this program is then
    // used by the `oom_handler_works` pallet test.
    WasmBuilder::new()
        .exclude_features(vec!["std"])
        .with_forced_recommended_toolchain() // NOTE: Don't use this in production programs!
        .build();
}
