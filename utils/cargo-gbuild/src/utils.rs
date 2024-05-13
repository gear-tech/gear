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

use anyhow::Result;
use std::path::PathBuf;

/// Collection crate manifests from the provided glob patterns.
pub fn collect_crates(patterns: &[String]) -> Result<Vec<PathBuf>> {
    let mut crates: Vec<PathBuf> = Default::default();
    for p in patterns {
        crates.append(
            &mut glob::glob(&p)?
                .filter_map(|p| {
                    p.ok().and_then(|p| {
                        let manifest = p.join("Cargo.toml");
                        if manifest.exists() {
                            Some(manifest)
                        } else {
                            tracing::warn!("Invalid manifest: {manifest:?}");
                            None
                        }
                    })
                })
                .collect(),
        );
    }

    Ok(crates)
}
