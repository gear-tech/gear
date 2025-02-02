// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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
