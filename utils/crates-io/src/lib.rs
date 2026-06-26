// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Crates.io manager library.
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

/// Local Polkadot SDK-compatible crates that Gear publishes under `g*` aliases.
///
/// NOTE: Each package in this array could possibly depend on the previous one,
/// please be cautious about changing the order.
pub const GEAR_SUBSTRATE_DEPENDENCIES: &[&str] = &[
    "sp-wasm-interface-common",
    "sp-allocator",
    "sp-wasm-interface",
    "sc-executor-common",
    "sc-executor-polkavm",
    "sc-executor-wasmtime",
    "substrate-wasm-builder",
];

/// Required Packages without local dependencies.
pub const SAFE_DEPENDENCIES: &[&str] = &[
    "actor-system-error",
    "galloc",
    "gbuiltin-bls381",
    "gear-common-codegen",
    "gear-core-errors",
    "gear-runtime-primitives",
    "gear-sandbox-env",
    "gear-ss58",
    "gear-stack-buffer",
    "gear-wasm-instrument",
    "gsdk-codegen",
    "gsys",
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
    "gear-wasmtime-cache",
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
    "ethexe-rpc-common",
    "ethexe-rpc-client",
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
pub const PACKAGE_ALIAS: [(&str, &str); 1] = [("gear-runtime-primitives", "runtime-primitives")];

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

/// Allowed categories for crates.io packages: https://crates.io/category_slugs.
pub const CRATES_IO_ALLOWED_CATEGORIES: &[&str] = &[
    "accessibility",
    "aerospace",
    "aerospace::drones",
    "aerospace::protocols",
    "aerospace::simulation",
    "aerospace::space-protocols",
    "aerospace::unmanned-aerial-vehicles",
    "algorithms",
    "api-bindings",
    "artificial-intelligence",
    "asynchronous",
    "authentication",
    "automotive",
    "caching",
    "command-line-interface",
    "command-line-utilities",
    "compilers",
    "compression",
    "computer-vision",
    "concurrency",
    "config",
    "cryptography",
    "cryptography::cryptocurrencies",
    "data-structures",
    "database",
    "database-implementations",
    "date-and-time",
    "development-tools",
    "development-tools::build-utils",
    "development-tools::cargo-plugins",
    "development-tools::debugging",
    "development-tools::ffi",
    "development-tools::procedural-macro-helpers",
    "development-tools::profiling",
    "development-tools::testing",
    "email",
    "embedded",
    "emulators",
    "encoding",
    "external-ffi-bindings",
    "filesystem",
    "finance",
    "game-development",
    "game-engines",
    "games",
    "graphics",
    "gui",
    "hardware-support",
    "internationalization",
    "localization",
    "mathematics",
    "memory-management",
    "multimedia",
    "multimedia::audio",
    "multimedia::encoding",
    "multimedia::images",
    "multimedia::video",
    "network-programming",
    "no-std",
    "no-std::no-alloc",
    "os",
    "os::android-apis",
    "os::freebsd-apis",
    "os::linux-apis",
    "os::macos-apis",
    "os::unix-apis",
    "os::windows-apis",
    "parser-implementations",
    "parsing",
    "rendering",
    "rendering::data-formats",
    "rendering::engine",
    "rendering::graphics-api",
    "rust-patterns",
    "science",
    "science::bioinformatics",
    "science::bioinformatics::genomics",
    "science::bioinformatics::proteomics",
    "science::bioinformatics::sequence-analysis",
    "science::computational-biology",
    "science::computational-biology::structural-modeling",
    "science::computational-biology::systems-biology",
    "science::computational-chemistry",
    "science::computational-chemistry::cheminformatics",
    "science::computational-chemistry::electronic-structure",
    "science::computational-chemistry::molecular-simulation",
    "science::geo",
    "science::materials",
    "science::neuroscience",
    "science::quantum-computing",
    "science::robotics",
    "security",
    "simulation",
    "template-engine",
    "text-editors",
    "text-processing",
    "value-formatting",
    "virtualization",
    "visualization",
    "wasm",
    "web-programming",
    "web-programming::http-client",
    "web-programming::http-server",
    "web-programming::websocket",
];
