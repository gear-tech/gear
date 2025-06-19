// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Utils for embedded commands.

use std::{fs, path::PathBuf};

/// This macro is used to lookup the artifact from the `OUT_DIR`,
/// it's just a wrapper around [`Artifact::from_out_dir`] for
/// avoiding importing [`Artifact`] in users' code.
///
/// NOTE: This macro should only be used in external crates.
#[macro_export]
macro_rules! lookup {
    () => {{
        if let Some(artifact) = ::gcli::embed::Artifact::from_out_dir(env!("OUT_DIR")) {
            artifact
        } else {
            ::gcli::log::warn!("No artifact is found from the out_dir");
            Default::default()
        }
    }};
}

/// The length of the suffix of the output folder.
///
/// Example: `[gcli]-1234567890abcdef`
const OUT_SUFFIX_LENGTH: usize = 17;

/// Program binaries for embedded commands.
#[derive(Debug, Default)]
pub struct Artifact {
    /// The optitmized WASM binary.
    pub opt: Vec<u8>,
}

impl Artifact {
    /// Parse the artifact from the `OUT_DIR`
    /// environment variable.
    pub fn from_out_dir(out: &str) -> Option<Self> {
        let out_dir = PathBuf::from(out);
        let mut ancestors = out_dir.ancestors();

        let [name, profile, target] = [
            ancestors
                .nth(1)?
                .file_name()?
                .to_str()
                .and_then(|name| name.get(..name.len().checked_sub(OUT_SUFFIX_LENGTH)?))?,
            if ancestors.nth(1)?.file_name()?.to_str()? == "debug" {
                "debug"
            } else {
                "release"
            },
            ancestors.next()?.to_str()?,
        ];

        let bin = PathBuf::from(format!("{target}/wasm32-gear/{profile}"));
        let stem = PathBuf::from(name.replace('-', "_"));

        Some(Self {
            opt: fs::read(bin.join(stem.with_extension("wasm"))).ok()?,
        })
    }
}
