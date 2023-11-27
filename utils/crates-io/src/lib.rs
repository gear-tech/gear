//! crates-io-manager library

mod manifest;
mod publisher;
pub mod rename;
mod version;

pub use self::{manifest::ManifestWithPath, publisher::Publisher, version::verify};
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

/// Packages need to be patched in dependencies.
pub const PATCHED_PACKAGES: [&str; 1] = ["sp-arithmetic"];

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
