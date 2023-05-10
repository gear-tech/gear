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

use anyhow::Result;
use std::{borrow::Cow, ffi::OsStr, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Toolchain(String);

impl Toolchain {
    /// Returns `Toolchain` representing the most recent nightly version.
    pub fn nightly() -> Self {
        Self("nightly".into())
    }
    /// Extracts `Toolchain` from cargo executable path.
    ///
    /// WARNING: There is no validation for the `path` argument provided.
    pub fn try_from_cargo_path(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();

        // Cargo path format:
        // "$RUSTUP_HOME/toolchains/**toolchain_name**/bin/cargo"
        let toolchain_name = path
            .iter()
            .nth_back(2)
            .and_then(OsStr::to_str)
            .ok_or_else(|| anyhow::anyhow!("Invalid cargo path: {}", path.display()))?;

        // Toolchain name format:
        // "**toolchain**-arch-arch-arch-arch"
        let toolchain = toolchain_name
            .rsplitn(5, '-')
            .last()
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("Invalid cargo toolchain name: {}", toolchain_name))?;

        Ok(Self(toolchain))
    }

    /// Returns toolchain string specification without target triple
    /// as it was passed during initialization.
    ///
    /// <channel>[-<date>]
    /// <channel> = stable|beta|nightly|<major.minor>|<major.minor.patch>
    /// <date>    = YYYY-MM-DD
    pub fn raw_toolchain_str(&'_ self) -> Cow<'_, str> {
        self.0.as_str().into()
    }

    /// Returns toolchain string specification without target triple
    /// with raw <channel> substituted by "nightly".
    ///
    /// nightly[-<date>]
    /// <date>    = YYYY-MM-DD
    pub fn nightly_toolchain_str(&'_ self) -> Cow<'_, str> {
        if !self.is_nightly() {
            let date_start_idx = self
                .0
                .find('-')
                .unwrap_or_else(|| self.raw_toolchain_str().len());
            let mut toolchain_str = self.0.clone();
            toolchain_str.replace_range(..date_start_idx, "nightly");
            toolchain_str.into()
        } else {
            self.raw_toolchain_str()
        }
    }

    /// Returns bool representing nightly toolchain.
    fn is_nightly(&self) -> bool {
        self.0.starts_with("nightly")
    }
}
