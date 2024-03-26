// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use std::env;

fn is_intellij_sync() -> bool {
    env::var("RUSTC_WRAPPER")
        .unwrap_or_default()
        .contains("intellij")
}

fn main() {
    let wasm_built = env::var("__GEAR_WASM_BUILT").as_deref() == Ok("1");

    if wasm_built || is_intellij_sync() {
        println!("cargo:rustc-cfg=cargo_gear");
    }

    if wasm_built {
        if let Ok(path) = env::var("__GEAR_WASM_TARGET_DIR") {
            println!("cargo:rustc-env=__GEAR_WASM_TARGET_DIR={path}");
        }
    }
}
