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
use std::{fmt, fmt::Display, fs, path::Path};

const LINEAR_COMPARISON_FILE_SIZE: u64 = 4096;

enum MaybeChangedData<'a> {
    Contents(&'a [u8]),
    Path(&'a Path),
}

impl MaybeChangedData<'_> {
    fn exists(&self) -> bool {
        match self {
            MaybeChangedData::Contents(_) => true,
            MaybeChangedData::Path(path) => path.exists(),
        }
    }

    fn len(&self) -> Result<u64> {
        Ok(match self {
            MaybeChangedData::Contents(contents) => contents.len() as u64,
            MaybeChangedData::Path(path) => fs::metadata(path)?.len(),
        })
    }

    fn check_linear_comparison_size(&self) -> Result<()> {
        let len = self.len()?;
        if len > LINEAR_COMPARISON_FILE_SIZE {
            // gear-wasm-builder doesn't write such big files
            unreachable!("{} is too large", self);
        }

        Ok(())
    }

    fn compare_content(&self, path: &Path) -> Result<bool> {
        self.check_linear_comparison_size()?;
        MaybeChangedData::Path(path).check_linear_comparison_size()?;

        let new = fs::read(path)?;

        let old_vec;
        let old = match *self {
            MaybeChangedData::Contents(contents) => contents,
            MaybeChangedData::Path(path) => {
                old_vec = fs::read(path)?;
                &old_vec
            }
        };

        Ok(new != old)
    }

    fn check_changed(self, path: &Path) -> Result<bool> {
        if !path.exists() {
            return Ok(true);
        }

        if !self.exists() {
            return Ok(true);
        }

        let metadata = fs::metadata(path)?;
        if metadata.len() != self.len()? {
            return Ok(true);
        }

        if self.compare_content(path)? {
            return Ok(true);
        }

        Ok(false)
    }
}

impl Display for MaybeChangedData<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MaybeChangedData::Contents(_) => write!(f, "Some content"),
            MaybeChangedData::Path(path) => write!(f, "{}", path.display()),
        }
    }
}

pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> Result<()> {
    let path = path.as_ref();
    let contents = contents.as_ref();
    if MaybeChangedData::Contents(contents).check_changed(path)? {
        fs::write(path, contents)?;
    }

    Ok(())
}

pub fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    if MaybeChangedData::Path(from).check_changed(to)? {
        fs::copy(from, to)?;
    }

    Ok(())
}
