//! crates-io-manager library

mod manifest;
mod publisher;
pub mod rename;
mod version;

pub use self::{manifest::ManifestWithPath, publisher::Publisher, version::verify};
use anyhow::Result;
use std::process::{Command, ExitStatus};

/// Packages need to be published.
pub const PACKAGES: [&str; 20] = [
    // Packages without local dependencies.
    "actor-system-error",
    "galloc",
    "gsys",
    "gear-stack-buffer",
    "gear-core-errors",
    "gear-common-codegen",
    "gear-wasm-instrument",
    "gmeta-codegen",
    "gsdk-codegen",
    "gstd-codegen",
    // The packages below have local dependencies,
    // and should be published in order.
    "gcore",
    "gmeta",
    "gstd",
    "gear-core",
    "gear-wasm-builder",
    "gear-utils",
    "gear-common",
    "gsdk",
    "gcli",
    "gclient",
];

/// Packages need to be patched in dependencies.
pub const PATCHED_PACKAGES: [&str; 1] = ["sp-arithmetic"];

/// Publish the input package
pub fn publish(manifest: &str) -> Result<ExitStatus> {
    Command::new("cargo")
        .arg("publish")
        .arg("-vv")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--allow-dirty")
        .status()
        .map_err(Into::into)
}
