// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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

#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

pub use gear_wasm_optimizer::{self as optimize, CargoCommand};
pub use wasm_project::{PreProcessor, PreProcessorResult, PreProcessorTarget};

use crate::wasm_project::WasmProject;
use anyhow::{Context, Result};
use gmeta::{Metadata, MetadataRepr};
use regex::Regex;
use std::{env, path::PathBuf, process};
use wasm_project::ProjectType;

mod builder_error;
pub mod code_validator;
mod crate_info;
mod multiple_crate_versions;
mod smart_fs;
mod wasm_project;

pub const TARGET: &str = env!("TARGET");

/// WASM building tool.
pub struct WasmBuilder {
    wasm_project: WasmProject,
    cargo: CargoCommand,
    excluded_features: Vec<&'static str>,
}

impl WasmBuilder {
    /// Create a new `WasmBuilder`.
    pub fn new() -> Self {
        WasmBuilder::create(WasmProject::new(ProjectType::Program(None)))
    }

    /// Create a new `WasmBuilder` for metawasm.
    pub fn new_metawasm() -> Self {
        WasmBuilder::create(WasmProject::new(ProjectType::Metawasm))
    }

    /// Create a new `WasmBuilder` with metadata.
    pub fn with_meta(metadata: MetadataRepr) -> Self {
        WasmBuilder::create(WasmProject::new(ProjectType::Program(Some(metadata))))
    }

    fn create(wasm_project: WasmProject) -> Self {
        WasmBuilder {
            wasm_project,
            cargo: CargoCommand::new(),
            excluded_features: vec![],
        }
    }

    /// Exclude features from the build.
    pub fn exclude_features(mut self, features: impl Into<Vec<&'static str>>) -> Self {
        self.excluded_features = features.into();
        self
    }

    /// Add pre-processor for wasm file.
    pub fn with_pre_processor(mut self, pre_processor: Box<dyn PreProcessor>) -> Self {
        self.wasm_project.add_preprocessor(pre_processor);
        self
    }

    /// Add check of recommended toolchain.
    pub fn with_recommended_toolchain(mut self) -> Self {
        self.cargo.set_check_recommended_toolchain(true);
        self
    }

    /// Force the recommended toolchain to be used, but do not check whether the
    /// current toolchain is recommended.
    ///
    /// NOTE: For internal use only, not recommended for production programs.
    ///
    /// An example usage can be found in `examples/out-of-memory/build.rs`.
    #[doc(hidden)]
    pub fn with_forced_recommended_toolchain(mut self) -> Self {
        self.cargo.set_force_recommended_toolchain(true);
        self
    }

    /// Build the program and produce an output WASM binary.
    ///
    /// Returns `None` if `__GEAR_WASM_BUILDER_NO_BUILD` flag is set.
    /// Returns `Some(_)` with a tuple of paths to wasm & opt wasm file
    /// if the build was successful.
    pub fn build(self) -> Option<(PathBuf, PathBuf)> {
        if env::var("__GEAR_WASM_BUILDER_NO_BUILD").is_ok() || is_intellij_sync() {
            _ = self.wasm_project.provide_dummy_wasm_binary_if_not_exist();
            return None;
        }

        match self.build_project() {
            Err(e) => {
                eprintln!("error: {e}");
                e.chain()
                    .skip(1)
                    .for_each(|cause| eprintln!("|      {cause}"));
                process::exit(1);
            }
            Ok(r) => r,
        }
    }

    fn build_project(mut self) -> Result<Option<(PathBuf, PathBuf)>> {
        self.wasm_project.generate()?;

        self.cargo
            .set_manifest_path(self.wasm_project.manifest_path());
        self.cargo.set_target_dir(self.wasm_project.target_dir());
        let profile = self.wasm_project.profile();
        let profile = if profile == "debug" { "dev" } else { profile };
        self.cargo.set_profile(profile.to_string());
        self.cargo.set_features(&self.enabled_features()?);
        if env::var("GEAR_WASM_BUILDER_PATH_REMAPPING").is_ok() {
            self.cargo.set_paths_to_remap(&self.paths_to_remap()?);
        }

        self.cargo.run()?;
        self.wasm_project.postprocess()
    }

    fn manifest_path(&self) -> Result<String> {
        let manifest_path = env::var("CARGO_MANIFEST_DIR")?;
        Ok(manifest_path)
    }

    /// Returns features enabled for the current build.
    fn enabled_features(&self) -> Result<Vec<String>> {
        let project_features = self.wasm_project.features();
        let enabled_features_iter = env::vars().filter_map(|(key, _)| {
            key.strip_prefix("CARGO_FEATURE_")
                .map(|feature| feature.to_lowercase())
        });
        let mut matched_features = Vec::new();
        let mut unmatched_features = Vec::new();
        for enabled_feature in enabled_features_iter {
            // Features coming via the CARGO_FEATURE_<feature> environment variable are in
            // normilized form, i.e. all dashes are replaced with underscores.
            let enabled_feature_regex =
                Regex::new(&format!("^{}$", enabled_feature.replace('_', "[-_]")))?;
            if self
                .excluded_features
                .iter()
                .any(|excluded_feature| enabled_feature_regex.is_match(excluded_feature))
            {
                continue;
            }
            if let Some(project_feature) = project_features
                .iter()
                .find(|project_feature| enabled_feature_regex.is_match(project_feature))
            {
                matched_features.push(project_feature.clone());
            } else {
                unmatched_features.push(enabled_feature);
            }
        }

        // It may turn out that crate with a build script is built as a dependency of
        // another crate with build script in the same process (runtime -> pallet-gear
        // -> examples). In that case, all the CARGO_FEATURE_<feature>
        // environment variables are propagated down to the dependent crate
        // which might not have the corresponding features at all.
        // In such situation, we just warn about unmatched features for diagnostic
        // purposes and ignore them as cargo itself checks initial set of
        // features before they reach the build script.
        if !unmatched_features.is_empty() && unmatched_features != ["default"] {
            println!(
                "cargo:warning=Package {}: features `{}` are not available and will be ignored",
                self.manifest_path()?,
                unmatched_features.join(", ")
            );
        }

        // NOTE: Filter out feature `gcli`.
        //
        // dependency feature `gcli` could be captured here
        // but it is not needed for the build.
        //
        // TODO: Filter dep features in this function (#3588)
        Ok(matched_features
            .into_iter()
            .filter(|feature| feature != "gcli")
            .collect())
    }

    fn paths_to_remap(&self) -> Result<Vec<(PathBuf, &'static str)>> {
        let home_dir = dirs::home_dir().context("unable to get home directory")?;

        let project_dir = self.wasm_project.original_dir();

        let cargo_dir = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .context("unable to get cargo home directory")?;

        let cargo_checkouts_dir = cargo_dir.join("git").join("checkouts");

        Ok(vec![
            (home_dir, "/home"),
            (project_dir, "/code"),
            (cargo_dir, "/cargo"),
            (cargo_checkouts_dir, "/deps"),
        ])
    }
}

impl Default for WasmBuilder {
    fn default() -> Self {
        Self::new()
    }
}

fn is_intellij_sync() -> bool {
    // Intellij Rust uses rustc wrapper during project sync
    env::var("RUSTC_WRAPPER")
        .unwrap_or_default()
        .contains("intellij")
}

// The `std` feature is excluded by default because it is usually used for
// building so called WASM wrapper which is a static library exposing the built
// WASM.
const FEATURES_TO_EXCLUDE_BY_DEFAULT: &[&str] = &["std"];

/// Shorthand function to be used in `build.rs`.
///
/// See [WasmBuilder::build()].
pub fn build() -> Option<(PathBuf, PathBuf)> {
    WasmBuilder::new()
        .exclude_features(FEATURES_TO_EXCLUDE_BY_DEFAULT.to_vec())
        .build()
}

/// Shorthand function to be used in `build.rs`.
///
/// See [WasmBuilder::build()].
pub fn build_with_metadata<T: Metadata>() -> Option<(PathBuf, PathBuf)> {
    WasmBuilder::with_meta(T::repr())
        .exclude_features(FEATURES_TO_EXCLUDE_BY_DEFAULT.to_vec())
        .build()
}

/// Shorthand function to be used in `build.rs`.
///
/// See [WasmBuilder::build()].
pub fn build_metawasm() -> Option<(PathBuf, PathBuf)> {
    WasmBuilder::new_metawasm()
        .exclude_features(FEATURES_TO_EXCLUDE_BY_DEFAULT.to_vec())
        .build()
}

/// Shorthand function to be used in `build.rs`.
///
/// See [WasmBuilder::build()].
pub fn recommended_nightly() -> Option<(PathBuf, PathBuf)> {
    WasmBuilder::new()
        .exclude_features(FEATURES_TO_EXCLUDE_BY_DEFAULT.to_vec())
        .with_recommended_toolchain()
        .build()
}

/// Shorthand function to be used in `build.rs`.
///
/// See [WasmBuilder::build()].
pub fn recommended_nightly_with_metadata<T: Metadata>() -> Option<(PathBuf, PathBuf)> {
    WasmBuilder::with_meta(T::repr())
        .exclude_features(FEATURES_TO_EXCLUDE_BY_DEFAULT.to_vec())
        .with_recommended_toolchain()
        .build()
}

/// Shorthand function to be used in `build.rs`.
///
/// See [WasmBuilder::build()].
pub fn recommended_nightly_metawasm() -> Option<(PathBuf, PathBuf)> {
    WasmBuilder::new_metawasm()
        .exclude_features(FEATURES_TO_EXCLUDE_BY_DEFAULT.to_vec())
        .with_recommended_toolchain()
        .build()
}
