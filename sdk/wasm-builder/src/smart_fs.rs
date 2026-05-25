// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Filesystem functions that avoid file changes
//! so we can avoid unnecessary build script launches.
//! because cargo looks for `mtime` metadata file parameter

use anyhow::Result;
use std::{fs, io::ErrorKind, path::Path};

pub(crate) fn copy_if_newer(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<bool> {
    let from = from.as_ref();
    let to = to.as_ref();

    if check_if_newer(from, to)? {
        fs::copy(from, to)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub(crate) fn check_if_newer(left: impl AsRef<Path>, right: impl AsRef<Path>) -> Result<bool> {
    let right_metadata = fs::metadata(right);

    if let Err(io_error) = right_metadata.as_ref()
        && io_error.kind() == ErrorKind::NotFound
    {
        return Ok(true);
    }

    let right_metadata = right_metadata.unwrap();
    let left_metadata = fs::metadata(left)?;

    Ok(left_metadata.modified()? > right_metadata.modified()?)
}

fn check_changed(path: &Path, contents: &[u8]) -> Result<bool> {
    // file does not exist
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(true);
    };

    if metadata.len() != contents.len() as u64 {
        return Ok(true);
    }

    let old_contents = fs::read(path)?;
    Ok(old_contents != contents)
}

pub(crate) fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> Result<()> {
    let path = path.as_ref();
    let contents = contents.as_ref();

    if check_changed(path, contents)? {
        fs::write(path, contents)?;
    }

    Ok(())
}
