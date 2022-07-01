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
use std::{env, process};

use crate::{cargo_command::CargoCommand, wasm_project::WasmProject};

mod builder_error;
mod cargo_command;
mod crate_info;
pub mod optimize;
mod stack_end;
mod wasm_project;

pub use optimize::check_exports;
pub use stack_end::insert_stack_end_export;

/// WASM building tool.
pub struct WasmBuilder {
    wasm_project: WasmProject,
    cargo: CargoCommand,
}

impl WasmBuilder {
    /// Create a new `WasmBuilder`.
    pub fn new() -> Self {
        WasmBuilder {
            wasm_project: WasmProject::new(),
            cargo: CargoCommand::new(),
        }
    }

    /// Build the program and produce an output WASM binary.
    pub fn build(self) {
        if env::var(self.cargo.skip_build_env()).is_ok() {
            return;
        }
        if let Err(e) = self.build_project() {
            eprintln!("error: {}", e);
            e.chain()
                .skip(1)
                .for_each(|cause| eprintln!("|      {}", cause));
            process::exit(1);
        }
    }

    fn build_project(mut self) -> Result<()> {
        // TODO: Check nightly toolchain
        self.wasm_project.generate()?;
        self.cargo
            .set_manifest_path(self.wasm_project.manifest_path());
        self.cargo.set_target_dir(self.wasm_project.target_dir());
        self.cargo
            .set_profile(self.wasm_project.profile().to_string());
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
