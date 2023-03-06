// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Gear program utils

use crate::result::Result;
use anyhow::anyhow;
use std::{fs, path::PathBuf};

/// home directory of cli `gear`
pub fn home() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into()).join(".gear");

    if !home.exists() {
        fs::create_dir_all(&home).expect("Failed to create ~/.gear");
    }

    home
}

pub fn hex_to_vec(string: impl AsRef<str>) -> Result<Vec<u8>> {
    hex::decode(string.as_ref().trim_start_matches("0x")).map_err(Into::into)
}

pub fn hex_to_hash(string: impl AsRef<str>) -> Result<[u8; 32]> {
    let hex = hex_to_vec(string)?;

    if hex.len() != 32 {
        return Err(anyhow!("Incorrect id length").into());
    }

    let mut arr = [0; 32];
    arr.copy_from_slice(&hex);

    Ok(arr)
}
