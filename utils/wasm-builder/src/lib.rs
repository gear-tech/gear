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
use gmeta::{Metadata, MetadataRepr};
use std::{env, process};
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
    pub fn build(self) {
        if env::var("__GEAR_WASM_BUILDER_NO_BUILD").is_ok() {
            return;
        }

        if let Err(e) = self.build_project() {
            eprintln!("error: {e}");
            e.chain()
                .skip(1)
                .for_each(|cause| eprintln!("|      {cause}"));
            process::exit(1);
        }
    }

    fn build_project(mut self) -> Result<()> {
        // TODO: Check nightly toolchain
        self.wasm_project.generate()?;
        self.cargo
            .set_manifest_path(self.wasm_project.manifest_path());
        self.cargo.set_target_dir(self.wasm_project.target_dir());

        let profile = self.wasm_project.profile();
        let profile = if profile == "debug" { "dev" } else { profile };
        self.cargo.set_profile(profile.to_string());
        self.cargo.run()?;
        self.wasm_project.postprocess()
    }
}

impl Default for WasmBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Shorthand function to be used in `build.rs`.
pub fn build() {
    WasmBuilder::new().build();
}

/// Shorthand function to be used in `build.rs`.
pub fn build_with_metadata<T: Metadata>() {
    WasmBuilder::with_meta(T::repr()).build();
}

/// Shorthand function to be used in `build.rs`.
pub fn build_metawasm() {
    WasmBuilder::new_metawasm().build();
}
