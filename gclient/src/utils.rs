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
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use wabt::Wat2Wasm;

/// Compile a source WebAssembly Text (WAT) to Wasm binary code.
pub fn wat2wasm(wat: impl AsRef<str>) -> Vec<u8> {
    Wat2Wasm::new()
        .convert(wat.as_ref())
        .expect("Failed to parse WAT")
        .as_ref()
        .to_vec()
}

/// Return the time elapsed since the Unix epoch in microseconds.
pub fn now_in_micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Infallible")
        .as_micros()
}

/// Return the time elapsed since the Unix epoch in microseconds converted to
/// the 16-bytes array.
#[deprecated = "Use `now_in_micros().to_le_bytes()` instead"]
pub fn bytes_now() -> [u8; 16] {
    now_in_micros().to_be_bytes()
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
