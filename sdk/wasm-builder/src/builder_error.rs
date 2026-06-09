// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use std::path::PathBuf;
use thiserror::Error;

/// Errors than can occur when building.
#[derive(Error, Debug)]
pub enum BuilderError {
    #[error("invalid manifest path `{0}`")]
    ManifestPathInvalid(PathBuf),

    #[error("please add \"rlib\" to [lib.crate-type]")]
    CrateTypeInvalid,

    #[error("unable to find the root package in cargo metadata")]
    RootPackageNotFound,
}
