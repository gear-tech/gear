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

//! crates-io-manager library
#![deny(missing_docs)]

mod manifest;
mod publisher;
mod version;

pub use self::{manifest::Manifest, publisher::Publisher, version::verify};
use anyhow::Result;
use std::process::{Command, ExitStatus};

/// Required Packages without local dependencies.
pub const SAFE_DEPENDENCIES: [&str; 10] = [
    "actor-system-error",
    "galloc",
    "gear-stack-buffer",
    "gear-core-errors",
    "gear-common-codegen",
    "gear-wasm-instrument",
    "gmeta-codegen",
    "gsdk-codegen",
    "gstd-codegen",
    "gsys",
];

/// Required packages with local dependencies.
pub const STACKED_DEPENDENCIES: [&str; 5] =
    ["gcore", "gmeta", "gear-core", "gear-utils", "gear-common"];

/// Packages need to be published.
pub const PACKAGES: [&str; 5] = ["gear-wasm-builder", "gstd", "gsdk", "gclient", "gcli"];

/// Check the input package
pub fn check(manifest: &str) -> Result<ExitStatus> {
    Command::new("cargo")
        .arg("check")
        .arg("--manifest-path")
        .arg(manifest)
        .status()
        .map_err(Into::into)
}

/// Publish the input package
pub fn publish(manifest: &str) -> Result<ExitStatus> {
    Command::new("cargo")
        .arg("publish")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--allow-dirty")
        .status()
        .map_err(Into::into)
}
