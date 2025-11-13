// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#[cfg(all(feature = "std", not(fuzz)))]
fn build_runtime() {
    let builder = substrate_wasm_builder::WasmBuilder::init_with_defaults()
        .append_to_cargo_flags(r#"--config=patch.crates-io.intentionally-empty.git="https://github.com/Kijewski/intentionally-empty""#);

    #[cfg(feature = "metadata-hash")]
    let builder = {
        const TOKEN_SYMBOL: &str = if cfg!(not(feature = "dev")) {
            "VARA"
        } else {
            "TVARA"
        };

        const DECIMALS: u8 = 12;

        builder.enable_metadata_hash(TOKEN_SYMBOL, DECIMALS)
    };

    builder.build()
}

#[cfg(all(feature = "std", not(fuzz)))]
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
    substrate_build_script_utils::generate_cargo_keys();

    #[cfg(all(feature = "std", not(fuzz)))]
    {
        skip_build_on_intellij_sync();
        build_runtime();
    }
}
