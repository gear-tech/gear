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
    "builtins-common",
    "gear-utils",
    "gear-common",
    "gear-wasmer-cache",
    "gear-sandbox-host",
    "gear-lazy-pages-common",
    "gear-lazy-pages",
    "gear-sandbox-interface",
    "gear-sandbox",
    "gear-core-backend",
    "gear-core-processor",
    "gear-lazy-pages-native-interface",
    "gsigner",
    "ethexe-common",
    "ethexe-ethereum",
    "ethexe-runtime-common",
    "ethexe-db",
    "ethexe-service-utils",
    "ethexe-observer",
    "ethexe-consensus",
    "ethexe-blob-loader",
    "ethexe-prometheus",
];

/// Packages need to be published.
///
/// NOTE: Each package in this array could possibly depend
/// on the previous one, please be cautious about changing
/// the order.
pub const PACKAGES: &[&str] = &[
    "gear-wasm-optimizer",
    "gear-wasm-builder",
    "gear-node-wrapper",
    "gtest",
    "cargo-gbuild",
    "gstd",
    "gsdk",
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

/// Add owner to the input package
pub fn add_owner(package: &str, owner: &str) -> Result<ExitStatus> {
    Command::new("cargo")
        .args(["+stable", "owner", "--add", owner, package])
        .status()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publish_index() -> Vec<&'static str> {
        [SAFE_DEPENDENCIES, STACKED_DEPENDENCIES, PACKAGES].concat()
    }

    #[test]
    fn publish_index_includes_publishable_ethexe_crates() {
        let index = publish_index();

        for package in [
            "ethexe-common",
            "ethexe-ethereum",
            "ethexe-runtime-common",
            "ethexe-db",
            "ethexe-service-utils",
            "ethexe-observer",
            "ethexe-consensus",
            "ethexe-blob-loader",
            "ethexe-prometheus",
        ] {
            assert!(index.contains(&package), "{package} must be publishable");
        }
    }

    #[test]
    fn publish_index_orders_ethexe_dependencies_before_gtest() {
        let index = publish_index();
        let gsigner_position = index
            .iter()
            .position(|package| *package == "gsigner")
            .expect("gsigner must be publishable");
        let gtest_position = index
            .iter()
            .position(|package| *package == "gtest")
            .expect("gtest must be publishable");

        for package in [
            "ethexe-common",
            "ethexe-ethereum",
            "ethexe-runtime-common",
            "ethexe-db",
            "ethexe-service-utils",
            "ethexe-observer",
            "ethexe-consensus",
            "ethexe-blob-loader",
            "ethexe-prometheus",
        ] {
            let package_position = index
                .iter()
                .position(|candidate| *candidate == package)
                .unwrap_or_else(|| panic!("{package} must be publishable"));

            assert!(
                gsigner_position < package_position,
                "gsigner must be published before {package}"
            );
            assert!(
                package_position < gtest_position,
                "{package} must be published before gtest"
            );
        }
    }

    #[test]
    fn publish_index_excludes_ethexe_crates_that_depend_on_processor_or_runtime() {
        let index = publish_index();

        for package in [
            "ethexe-processor",
            "ethexe-runtime",
            "ethexe-network",
            "ethexe-rpc",
            "ethexe-node-wrapper",
            "ethexe-sdk",
            "ethexe-compute",
            "ethexe-service",
            "ethexe-cli",
            "ethexe-node-loader",
        ] {
            assert!(
                !index.contains(&package),
                "{package} must not be publishable"
            );
        }
    }
}
