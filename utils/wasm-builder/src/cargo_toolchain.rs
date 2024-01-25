// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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

use crate::builder_error::BuilderError;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{borrow::Cow, process::Command};

// The channel patterns we support (borrowed from the rustup code)
static TOOLCHAIN_CHANNELS: &[&str] = &[
    "nightly",
    "beta",
    "stable",
    // Allow from 1.0.0 through to 9.999.99 with optional patch version
    r"\d{1}\.\d{1,3}(?:\.\d{1,2})?",
];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Toolchain(String);

impl Toolchain {
    /// This is a version of nightly toolchain, tested on our CI.
    const PINNED_NIGHTLY_TOOLCHAIN: &'static str = "nightly-2023-09-05";

    /// Returns `Toolchain` representing the recommended nightly version.
    pub fn recommended_nightly() -> Self {
        Self(Self::PINNED_NIGHTLY_TOOLCHAIN.into())
    }

    /// Fetches `Toolchain` via rustup.
    pub fn try_from_rustup() -> Result<Self> {
        let output = Command::new("rustup")
            .args(["show", "active-toolchain"])
            .output()
            .context("`rustup` command failed")?;

        anyhow::ensure!(
            output.status.success(),
            "`rustup` exit code is not successful"
        );

        let toolchain_desc = output
            .stdout
            .split(|&x| x == b' ')
            .next()
            .and_then(|s| std::str::from_utf8(s).ok())
            .expect("unexpected `rustup` output");

        // TODO #3499: replace it with `std::sync::LazyLock` when it becomes stable
        static TOOLCHAIN_CHANNEL_RE: Lazy<Regex> = Lazy::new(|| {
            // This regex is borrowed from the rustup code and modified (added non-capturing groups)
            let pattern = format!(
                r"^((?:{})(?:-(?:\d{{4}}-\d{{2}}-\d{{2}}))?)(?:-(?:.+))?$",
                TOOLCHAIN_CHANNELS.join("|")
            );
            // Note this regex gives you a guaranteed match of the channel[-date] as group 1
            Regex::new(&pattern).unwrap()
        });

        let toolchain = TOOLCHAIN_CHANNEL_RE
            .captures(toolchain_desc)
            .ok_or_else(|| BuilderError::CargoToolchainInvalid(toolchain_desc.into()))?
            .get(1)
            .unwrap() // It is safe to use unwrap here because we know the regex matches
            .as_str()
            .to_owned();

        Ok(Self(toolchain))
    }

    /// Returns toolchain string specification without target triple
    /// as it was passed during initialization.
    ///
    /// `<channel>[-<date>]`
    ///
    /// `<channel> = stable|beta|nightly|<major.minor>|<major.minor.patch>`
    ///
    /// `<date>    = YYYY-MM-DD`
    pub fn raw_toolchain_str(&'_ self) -> Cow<'_, str> {
        self.0.as_str().into()
    }

    /// Checks whether the toolchain is recommended.
    pub fn check_recommended_toolchain(&self) -> Result<()> {
        let toolchain = Self::PINNED_NIGHTLY_TOOLCHAIN;
        anyhow::ensure!(
            self.raw_toolchain_str() == toolchain,
            BuilderError::RecommendedToolchainNotFound(toolchain.into()),
        );
        Ok(())
    }
}
