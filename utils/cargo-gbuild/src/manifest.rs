// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use serde::Deserialize;
use std::path::PathBuf;

/// Cargo manifest abstraction.
///
/// NOTE: considering using `cargo-edit` instead in the future.
#[derive(Debug, Deserialize)]
pub struct Manifest {
    /// The build section in the cargo manifest.
    pub build: Option<Build>,

    /// The package section in the cargo manifest.
    pub package: Package,
}

/// The package section in the cargo manifest.
#[derive(Debug, Deserialize)]
pub struct Package {
    /// Name of the package
    pub name: String,
}

/// The build section in the cargo manifest.
#[derive(Debug, Deserialize)]
pub struct Build {
    /// The target directory of the cargo project.
    pub target_dir: Option<PathBuf>,
}
