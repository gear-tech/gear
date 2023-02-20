// This file is part of Gear.
//
// Copyright (C) 2023 Gear Technologies Inc.
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

//! Filesystem functions that avoid file changes
//! so we can avoid unnecessary build script launches.
//! because cargo looks for `mtime` metadata file parameter

use anyhow::Result;
use std::{fs, path::Path};

const LINEAR_COMPARISON_FILE_SIZE: u64 = 4096;

fn check_changed(path: &Path, contents: &[u8]) -> Result<bool> {
    let metadata = fs::metadata(path)?;
    if metadata.len() != contents.len() as u64 {
        return Ok(true);
    }

    if metadata.len() > LINEAR_COMPARISON_FILE_SIZE {
        // gear-wasm-builder doesn't write such big files
        unreachable!("{} is too large", path.display());
    } else {
        let file = fs::read(path)?;
        if file != contents {
            return Ok(true);
        }
    }

    Ok(false)
}

pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> Result<()> {
    let path = path.as_ref();
    let contents = contents.as_ref();
    if check_changed(path, contents)? {
        fs::write(path, contents)?;
    }

    Ok(())
}

pub fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    let contents = fs::read(from)?;
    if check_changed(to, &contents)? {
        fs::copy(from, to)?;
    }

    Ok(())
}
