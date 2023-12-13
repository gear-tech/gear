// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! Renaming handler

use anyhow::Result;
use cargo_metadata::Package;
use cargo_toml::{Dependency, Manifest};

/// Rename a package
pub fn package(pkg: &Package, manifest: &mut Manifest) -> Result<()> {
    // NOTE: This is a bug inside of crate cargo_toml, it should
    // not append crate-type = ["rlib"] to proc-macro crates, fixing
    // it by hacking it now.
    if pkg.name.ends_with("-codegen") {
        if let Some(product) = manifest.lib.as_mut() {
            product.crate_type = vec![];
        }
    }

    Ok(())
}

/// Rename a dependencies
pub fn deps(map: &mut Manifest, index: Vec<&String>, version: String) -> Result<()> {
    for (name, dep) in map.dependencies.iter_mut() {
        // No need to update dependencies for packages without
        // local dependencies.
        if !index.contains(&name) && !name.starts_with("sp-") {
            continue;
        }

        let mut detail = if let Dependency::Detailed(detail) = &dep {
            detail.clone()
        } else {
            continue;
        };

        match name.as_ref() {
            // NOTE: the required version of sp-arithmetic is 6.0.0 in
            // git repo, but 7.0.0 in crates.io, so we need to fix it.
            "sp-arithmetic" => {
                detail.version = Some("7.0.0".into());
                detail.branch = None;
                detail.git = None;
            }
            sp if sp.starts_with("sp-") => {
                detail.branch = None;
                detail.git = None;
            }
            _ => detail.version = Some(version.clone()),
        }

        *dep = Dependency::Detailed(detail);
    }

    Ok(())
}
