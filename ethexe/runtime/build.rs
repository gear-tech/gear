// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#[cfg(feature = "std")]
fn skip_build_on_intellij_sync() {
    // Intellij Rust uses rustc wrapper during project sync
    let is_intellij = std::env::var("RUSTC_WRAPPER")
        .unwrap_or_default()
        .contains("intellij");
    if is_intellij {
        unsafe { std::env::set_var("SKIP_WASM_BUILD", "1") }
    }
}

fn main() {
    #[cfg(feature = "std")]
    {
        skip_build_on_intellij_sync();
        substrate_wasm_builder::WasmBuilder::new()
            .with_current_project()
            .disable_runtime_version_section_check()
            .append_to_cargo_flags(
                r#"--config=patch.crates-io.gear-workspace-hack.registry="crates-io-patch-hack""#,
            )
            .append_to_cargo_flags(
                r#"--config=patch.crates-io.gear-workspace-hack.version="0.1.0""#,
            )
            .build();
    }
}
