// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
    "ethexe-rpc",
];

/// Packages need to be published.
///
/// NOTE: Each package in this array could possibly depend
/// on the previous one, please be cautious about changing
/// the order.
pub const PACKAGES: &[&str] = &[
    "ethexe-node-wrapper",
    "ethexe-sdk",
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
