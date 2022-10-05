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
use futures_timer::Delay;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use wabt::Wat2Wasm;

pub fn wat2wasm(wat: impl AsRef<str>) -> Vec<u8> {
    Wat2Wasm::new()
        .convert(wat.as_ref())
        .expect("Failed to parse WASM")
        .as_ref()
        .to_vec()
}

pub fn bytes_now() -> [u8; 16] {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Infallible")
        .as_micros()
        .to_le_bytes()
}

pub fn wasm_target(root_path: impl AsRef<str>, demo_name: impl AsRef<str>) -> String {
    format!(
        "{}/target/wasm32-unknown-unknown/release/{}.opt.wasm",
        root_path.as_ref(),
        demo_name.as_ref()
    )
}

pub fn code_from_os(path: impl Into<PathBuf>) -> Result<Vec<u8>> {
    let path = path.into();

    path.as_path()
        .extension()
        .and_then(|extension| extension.eq("wasm").then_some(()))
        .ok_or(Error::WrongBinaryExtension)?;

    let path = fs::canonicalize(path)?;
    fs::read(path).map_err(Into::into)
}

pub fn wait_task(millis: u64) -> Delay {
    Delay::new(Duration::from_millis(millis))
}
