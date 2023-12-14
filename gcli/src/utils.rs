// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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
use std::{env, fs, path::PathBuf};

/// home directory of cli `gear`
pub fn home() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into()).join(".gear");

    if !home.exists() {
        fs::create_dir_all(&home).expect("Failed to create ~/.gear");
    }

    home
}

/// Parse the metadata path with result.
pub fn meta_path(meta: Option<PathBuf>, opt: &str) -> Result<PathBuf> {
    if let Some(meta) = meta {
        return Ok(meta);
    }

    let cwd = env::current_dir()?;
    for entry in fs::read_dir(&cwd)? {
        let file = entry?.path();
        if file.ends_with(".meta.wasm") {
            return Ok(file);
        }
    }

    Err(anyhow!(
        "Could not find any *.meta.wasm in {cwd:?}, please specify the metadata path with --{opt}",
    )
    .into())
}

pub trait Hex {
    fn to_vec(&self) -> Result<Vec<u8>>;
    fn to_hash(&self) -> Result<[u8; 32]>;
}

impl<T> Hex for T
where
    T: AsRef<str>,
{
    fn to_vec(&self) -> Result<Vec<u8>> {
        hex::decode(self.as_ref().trim_start_matches("0x")).map_err(Into::into)
    }

    fn to_hash(&self) -> Result<[u8; 32]> {
        let hex = self.to_vec()?;

        if hex.len() != 32 {
            return Err(anyhow!("Incorrect id length").into());
        }

        let mut arr = [0; 32];
        arr.copy_from_slice(&hex);

        Ok(arr)
    }
}
