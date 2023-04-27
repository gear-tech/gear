// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use anyhow::Result;
use filetime::{set_file_mtime, FileTime};
use gmeta::{Metadata, MetadataRepr};
use std::{env, path::Path, process};
use wasm_project::ProjectType;

use crate::{cargo_command::CargoCommand, wasm_project::WasmProject};

mod builder_error;
mod cargo_command;
mod crate_info;
pub mod optimize;
mod smart_fs;
mod stack_end;
mod wasm_project;

pub use stack_end::insert_stack_end_export;

pub const TARGET: &str = env!("TARGET");

/// WASM building tool.
pub struct WasmBuilder {
    wasm_project: WasmProject,
    cargo: CargoCommand,
}

impl WasmBuilder {
    /// Create a new `WasmBuilder`.
    pub fn new() -> Self {
        WasmBuilder {
            wasm_project: WasmProject::new(ProjectType::Program(None)),
            cargo: CargoCommand::new(),
        }
    }

    /// Create a new `WasmBuilder` for metawasm.
    pub fn new_metawasm() -> Self {
        WasmBuilder {
            wasm_project: WasmProject::new(ProjectType::Metawasm),
            cargo: CargoCommand::new(),
        }
    }

    /// Create a new `WasmBuilder` with metadata.
    pub fn with_meta(metadata: MetadataRepr) -> Self {
        WasmBuilder {
            wasm_project: WasmProject::new(ProjectType::Program(Some(metadata))),
            cargo: CargoCommand::new(),
        }
    }

    /// Build the program and produce an output WASM binary.
    pub fn build(self, features_to_exclude: &[&str]) {
        if env::var("__GEAR_WASM_BUILDER_NO_BUILD").is_ok() {
            return;
        }

        if let Err(e) = self.build_project(features_to_exclude) {
            eprintln!("error: {e}");
            e.chain()
                .skip(1)
                .for_each(|cause| eprintln!("|      {cause}"));
            process::exit(1);
        }

        WasmBuilder::force_build_script_rebuild_on_next_run();
    }

    fn build_project(mut self, features_to_exclude: &[&str]) -> Result<()> {
        // TODO: Check nightly toolchain
        self.wasm_project.generate()?;

        self.cargo
            .set_manifest_path(self.wasm_project.manifest_path());
        self.cargo.set_target_dir(self.wasm_project.target_dir());
        let profile = self.wasm_project.profile();
        let profile = if profile == "debug" { "dev" } else { profile };
        self.cargo.set_profile(profile.to_string());
        self.cargo.set_features(&WasmBuilder::enabled_features(
            &self.wasm_project,
            features_to_exclude,
        )?);

        self.cargo.run()?;
        self.wasm_project.postprocess()
    }

    /// Returns the features enabled for the current build.
    fn enabled_features(
        wasm_project: &WasmProject,
        features_to_exclude: &[&str],
    ) -> Result<Vec<String>> {
        let features = env::vars()
            .filter_map(|(key, _)| {
                key.strip_prefix("CARGO_FEATURE_")
                    .map(|feature| feature.to_lowercase())
            })
            .filter(|feature| {
                // Omit the `default` feature because the generated project excludes it
                feature != "default" && !features_to_exclude.contains(&feature.as_str())
            })
            .collect::<Vec<String>>();
        wasm_project.match_features(&features)
    }

    /// Force cargo to rebuild the build script next time it is invoked (e.g. when a feature is added or removed).
    fn force_build_script_rebuild_on_next_run() {
        let build_rs_path = Path::new(
            &env::var("CARGO_MANIFEST_DIR")
                .expect("Failed to read the CARGO_MANIFEST_DIR variable"),
        )
        .join("build.rs");
        set_file_mtime(build_rs_path, FileTime::now())
            .expect("Failed to update the build script modification time");
    }
}

impl Default for WasmBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// The `std` feature is excluded by default because it is usually used for building
// so called WASM wrapper which is a static library exposing the built WASM.
const FEATURES_TO_EXCLUDE_BY_DEFAULT: &[&str] = &["std"];

/// Shorthand function to be used in `build.rs`.
pub fn build() {
    build_custom(FEATURES_TO_EXCLUDE_BY_DEFAULT)
}

/// Shorthand function similar to the [`build`] one, but allowing some customizations.
pub fn build_custom(features_to_exclude: &[&str]) {
    WasmBuilder::new().build(features_to_exclude);
}

/// Shorthand function to be used in `build.rs`.
pub fn build_with_metadata<T: Metadata>() {
    build_with_metadata_custom::<T>(FEATURES_TO_EXCLUDE_BY_DEFAULT);
}

/// Shorthand function similar to the [`build_with_metadata`] one, but allowing some customizations.
pub fn build_with_metadata_custom<T: Metadata>(features_to_exclude: &[&str]) {
    WasmBuilder::with_meta(T::repr()).build(features_to_exclude);
}

/// Shorthand function to be used in `build.rs`.
pub fn build_metawasm() {
    build_metawasm_custom(FEATURES_TO_EXCLUDE_BY_DEFAULT);
}

/// Shrthand function similar to the [`build_metawasm`] one, but allowing some customizations.
pub fn build_metawasm_custom(features_to_exclude: &[&str]) {
    WasmBuilder::new_metawasm().build(features_to_exclude);
}
