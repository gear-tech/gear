// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use anyhow::{Context, Result, anyhow, ensure};
use regex::Regex;
use std::{borrow::Cow, process::Command, sync::LazyLock};

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
    const PINNED_NIGHTLY_TOOLCHAIN: &'static str = "nightly-2025-09-12";

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

        ensure!(
            output.status.success(),
            "`rustup` exit code is not successful"
        );

        let toolchain_desc =
            std::str::from_utf8(&output.stdout).expect("unexpected `rustup` output");

        static TOOLCHAIN_CHANNEL_RE: LazyLock<Regex> = LazyLock::new(|| {
            let channels = TOOLCHAIN_CHANNELS.join("|");
            let pattern = format!(r"(?:{channels})(?:-\d{{4}}-\d{{2}}-\d{{2}})?");
            // Note this regex gives you a guaranteed match of the channel[-date] as group 0,
            // for example: `nightly-2025-09-12`
            Regex::new(&pattern).unwrap()
        });

        let toolchain = TOOLCHAIN_CHANNEL_RE
            .captures(toolchain_desc)
            .ok_or_else(|| anyhow!("cargo toolchain is invalid {toolchain_desc}"))?
            .get(0)
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
        ensure!(
            self.raw_toolchain_str() == toolchain,
            anyhow!(
                "recommended toolchain `{x}` not found, install it using the command:\n\
        rustup toolchain install {x} --target wasm32v1-none\n\n\
        after installation, do not forget to set `channel = \"{x}\"` in `rust-toolchain.toml` file",
                x = toolchain
            )
        );
        Ok(())
    }
}
