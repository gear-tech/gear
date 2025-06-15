// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! environment paths and binaries

use std::env;

/// target path from the root workspace
const TARGET: &str = "target";
const WASM_TARGET: &str = "target/wasm32-gear";

pub const PROFILE: &str = if cfg!(debug_assertions) {
    "debug"
} else {
    "release"
};

fn bin_path(name: &str, profile: &str, wasm: bool) -> String {
    format!(
        "{manifest_dir}/../{target_dir}/{profile}/{name}",
        manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap(),
        target_dir = if wasm { WASM_TARGET } else { TARGET }
    )
}

/// path of gear node binary
pub fn node_bin() -> String {
    bin_path("gear", "release", false)
}

/// path of binaries
pub fn bin(name: &str) -> String {
    bin_path(name, PROFILE, false)
}
