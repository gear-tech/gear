// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
mod simulator;
mod version;

pub use self::{
    manifest::{LockFile, Manifest, Workspace},
    publisher::Publisher,
    simulator::Simulator,
    version::{PackageStatus, verify, verify_owners},
};
use anyhow::Result;
use std::process::{Command, ExitStatus};

/// Username that owns crates.
pub const USER_OWNER: &str = "breathx";

/// Team that owns crates.
pub const TEAM_OWNER: &str = "github:gear-tech:dev";

/// Expected owners of crates.
pub const EXPECTED_OWNERS: [&str; 2] = [USER_OWNER, TEAM_OWNER];

/// Required Packages without local dependencies.
pub const SAFE_DEPENDENCIES: &[&str] = &[
    "actor-system-error",
    "galloc",
    "gear-ss58",
    "gear-stack-buffer",
    "gear-core-errors",
    "gear-common-codegen",
    "gear-runtime-primitives",
    "gear-sandbox-env",
    "gear-wasm-instrument",
    "gsdk-codegen",
    "gsys",
    "numerated",
    "gbuiltin-bls381",
];

/// Required packages with local dependencies.
///
/// NOTE: Each package in this array could possibly depend
/// on the previous one, please be cautious about changing
/// the order.
pub const STACKED_DEPENDENCIES: &[&str] = &[
    "gprimitives",
    "gbuiltin-eth-bridge",
    "pallet-gear-eth-bridge-primitives",
    "gbuiltin-proxy",
    "gbuiltin-staking",
    "gstd-codegen",
    "gcore",
    "gear-core",
    "gear-utils",
    "gear-common",
    "gear-wasmer-cache",
    "gear-sandbox-host",
    "gear-lazy-pages-common",
    "gear-lazy-pages",
    "gear-sandbox-interface",
    "gear-sandbox",
    "gear-core-backend",
    "gear-lazy-pages-native-interface",
    "gear-core-processor",
];

/// Packages need to be published.
///
/// NOTE: Each package in this array could possibly depend
/// on the previous one, please be cautious about changing
/// the order.
pub const PACKAGES: &[&str] = &[
    "gring",
    "gear-wasm-optimizer",
    "gear-wasm-builder",
    "gear-node-wrapper",
    "gtest",
    "cargo-gbuild",
    "gstd",
    "gsdk",
    "gclient",
    "gcli",
    "wasm-proc",
];

/// Alias for packages.
pub const PACKAGE_ALIAS: [(&str, &str); 2] = [
    ("gear-core-processor", "core-processor"),
    ("gear-runtime-primitives", "runtime-primitives"),
];

/// Name for temporary cargo registry.
pub const CARGO_REGISTRY_NAME: &str = "cargo-http-registry";

/// Test the input package
pub fn test(package: &str, test: &str) -> Result<ExitStatus> {
    Command::new("cargo")
        .args(["+1.88.0", "test", "-p", package, "--", test])
        .status()
        .map_err(Into::into)
}

/// Publish the input package
pub fn publish(manifest: &str) -> Result<ExitStatus> {
    Command::new("cargo")
        .args([
            "+1.88.0",
            "publish",
            "--manifest-path",
            manifest,
            "--allow-dirty",
        ])
        .status()
        .map_err(Into::into)
}

/// Add owner to the input package
pub fn add_owner(package: &str, owner: &str) -> Result<ExitStatus> {
    Command::new("cargo")
        .args(["+1.88.0", "owner", "--add", owner, package])
        .status()
        .map_err(Into::into)
}
