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

use crate::Manifest;
use anyhow::Result;

/// Rename a dependencies
pub fn deps(pkg: &mut Manifest, index: Vec<&str>, version: &str) -> Result<()> {
    let Some(deps) = pkg.manifest["dependencies"].as_table_like_mut() else {
        return Ok(());
    };

    for (name, dep) in deps.iter_mut() {
        let name = name.get();
        if !index.contains(&name) && !name.starts_with("sp-") {
            continue;
        }

        let Some(dep) = dep.as_table_like_mut() else {
            continue;
        };

        match name {
            // NOTE: the required version of sp-arithmetic is 6.0.0 in
            // git repo, but 7.0.0 in crates.io, so we need to fix it.
            "sp-arithmetic" => {
                dep.insert(version, toml_edit::value("7.0.0"));
                dep.remove("branch");
                dep.remove("git")
            }
            sp if sp.starts_with("sp-") => {
                dep.remove("branch");
                dep.remove("git")
            }
            _ => dep.insert(version, toml_edit::value("7.0.0")),
        };
    }

    Ok(())
}
