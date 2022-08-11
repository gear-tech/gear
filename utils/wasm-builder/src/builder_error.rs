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

use std::path::PathBuf;
use thiserror::Error;

/// Errors than can occur when building.
#[derive(Error, Debug)]
pub enum BuilderError {
    #[error("invalid manifest path `{0}`")]
    InvalidManifestPath(PathBuf),

    #[error("please add \"rlib\" to [lib.crate-type]")]
    InvalidCrateType,

    #[error("cargo command run failed: {0}")]
    CargoRunFailed(String),

    #[error("unable to find the root package in cargo metadata")]
    RootPackageNotFound,

    #[error("WASM module does't contain export section `{0}`")]
    ExportSectionNotFound(PathBuf),

    #[error(
        "WASM module doesn't contain required export funtions (init/handle) after optimized `{0}`"
    )]
    RequiredExportFnNotFound(PathBuf),
}
