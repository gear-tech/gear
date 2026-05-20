// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gear_wasm_builder::WasmBuilder;

fn main() {
    WasmBuilder::new()
        .exclude_features(vec!["std", "wasm-wrapper"])
        .build();
}
