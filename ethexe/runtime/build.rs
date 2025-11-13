// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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
            .append_to_cargo_flags(r#"--config=patch.crates-io.intentionally-empty.git="https://github.com/Kijewski/intentionally-empty""#)
            .build();
    }
}
