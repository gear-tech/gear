// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gear_wasm_builder::WasmBuilder;

fn main() {
    // This program exercises `#[alloc_error_handler]`, which is still nightly-only.
    // The WASM binary is used by the `oom_handler_works` pallet test.
    WasmBuilder::new()
        .exclude_features(vec!["std"])
        .with_forced_nightly_toolchain()
        .build();
}
