// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

mod handler;
mod manifest;
mod publisher;
mod version;

pub use self::{manifest::Manifest, publisher::Publisher, version::verify};
use anyhow::Result;
use std::process::{Command, ExitStatus};

/// Required Packages without local dependencies.
pub const SAFE_DEPENDENCIES: [&str; 15] = [
    "actor-system-error",
    "galloc",
    "gprimitives",
    "gear-ss58",
    "gear-stack-buffer",
    "gear-core-errors",
    "gear-common-codegen",
    "gear-runtime-primitives",
    "gear-sandbox-env",
    "gear-wasm-instrument",
    "gmeta-codegen",
    "gsdk-codegen",
    "gstd-codegen",
    "gsys",
    "numerated",
];

/// Required packages with local dependencies.
///
/// NOTE: Each package in this array could possibly depend
/// on the previous one, please be cautious about changing
/// the order.
pub const STACKED_DEPENDENCIES: [&str; 14] = [
    "gcore",
    "gmeta",
    "gear-core",
    "gear-utils",
    "gear-common",
    "gear-sandbox-host",
    "gear-lazy-pages-common",
    "gear-lazy-pages",
    "gear-runtime-interface",
    "gear-lazy-pages-interface",
    "gear-sandbox",
    "gear-core-backend",
    "gear-core-processor",
    "gear-lazy-pages-native-interface",
];

/// Packages need to be published.
///
/// NOTE: Each package in this array could possibly depend
/// on the previous one, please be cautious about changing
/// the order.
pub const PACKAGES: [&str; 9] = [
    "gring",
    "gear-wasm-builder",
    "gear-node-wrapper",
    "cargo-gbuild",
    "gstd",
    "gtest",
    "gsdk",
    "gclient",
    "gcli",
];

/// Alias for packages.
pub const PACKAGE_ALIAS: [(&str, &str); 2] = [
    ("gear-core-processor", "core-processor"),
    ("gear-runtime-primitives", "runtime-primitives"),
];

/// Check the input package
pub fn check(manifest: &str) -> Result<ExitStatus> {
    Command::new("cargo")
        .args(["+stable", "check", "--lib", "--manifest-path", manifest])
        .status()
        .map_err(Into::into)
}

/// Test the input package
pub fn test(package: &str, test: &str) -> Result<ExitStatus> {
    Command::new("cargo")
        .args(["+stable", "test", "-p", package, "--", test])
        .status()
        .map_err(Into::into)
}

/// Publish the input package
pub fn publish(manifest: &str) -> Result<ExitStatus> {
    Command::new("cargo")
        .args([
            "+stable",
            "publish",
            "--manifest-path",
            manifest,
            "--allow-dirty",
        ])
        .status()
        .map_err(Into::into)
}
