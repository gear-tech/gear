// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use crate::{Error, Result};
pub use gear_utils::now_micros;
use std::{fs, path::Path};
use wabt::Wat2Wasm;

/// Compile a source WebAssembly Text (WAT) to Wasm binary code.
pub fn wat2wasm(wat: impl AsRef<str>) -> Vec<u8> {
    Wat2Wasm::new()
        .convert(wat.as_ref())
        .expect("Failed to parse WAT")
        .as_ref()
        .to_vec()
}

/// Return the full path to the optimized Wasm binary file with the `demo_name`
/// name located in the `root_path` directory.
///
/// # Examples
///
/// ```
/// let wasm_path = gclient::wasm_target(".", "demo_ping");
/// assert_eq!(
///     wasm_path,
///     "./target/wasm32-unknown-unknown/release/demo_ping.opt.wasm"
/// );
/// ```
pub fn wasm_target(root_path: impl AsRef<str>, demo_name: impl AsRef<str>) -> String {
    format!(
        "{}/target/wasm32-unknown-unknown/release/{}.opt.wasm",
        root_path.as_ref(),
        demo_name.as_ref()
    )
}

/// Read and return contents of a Wasm file specified by the `path`.
pub fn code_from_os(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    path.as_ref()
        .extension()
        .filter(|&extension| extension.eq("wasm"))
        .ok_or(Error::WrongBinaryExtension)?;

    fs::read(fs::canonicalize(path)?).map_err(Into::into)
}

/// Convert hex string to byte array.
pub fn hex_to_vec(string: impl AsRef<str>) -> Result<Vec<u8>> {
    hex::decode(string.as_ref().trim_start_matches("0x")).map_err(Into::into)
}
