// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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

use colored::Colorize;

use crate::stack_end;
use anyhow::{anyhow, Context, Result};
use gear_wasm_instrument::{Module, STACK_END_EXPORT_NAME};
use std::{
    fs::{self, metadata},
    path::{Path, PathBuf},
    process::Command,
};

pub const FUNC_EXPORTS: [&str; 4] = ["init", "handle", "handle_reply", "handle_signal"];

const OPTIMIZED_EXPORTS: [&str; 7] = [
    "handle",
    "handle_reply",
    "handle_signal",
    "init",
    "state",
    "metahash",
    STACK_END_EXPORT_NAME,
];

pub struct Optimizer {
    module: Module,
}

impl Optimizer {
    pub fn new(file: &PathBuf) -> Result<Self> {
        let contents = fs::read(file)
            .with_context(|| format!("Failed to read file by optimizer: {file:?}"))?;
        let module = Module::new(&contents).with_context(|| format!("File path: {file:?}"))?;
        Ok(Self { module })
    }

    pub fn insert_start_call_in_export_funcs(&mut self) -> Result<(), &'static str> {
        stack_end::insert_start_call_in_export_funcs(&mut self.module)
    }

    pub fn move_mut_globals_to_static(&mut self) -> Result<(), &'static str> {
        stack_end::move_mut_globals_to_static(&mut self.module)
    }

    pub fn insert_stack_end_export(&mut self) -> Result<(), &'static str> {
        stack_end::insert_stack_end_export(&mut self.module)
    }

    /// Strips all custom sections.
    ///
    /// Presently all custom sections are not required so they can be stripped
    /// safely. The name section is already stripped by `wasm-opt`.
    pub fn strip_custom_sections(&mut self) {
        // we also should strip `reloc` section
        // if it will be present in the module in the future
        self.module.custom_sections = None;
        self.module.name_section = None;
    }

    /// Keeps only allowlisted exports.
    pub fn strip_exports(&mut self) {
        if let Some(export_section) = self.module.export_section.as_mut() {
            let exports = OPTIMIZED_EXPORTS.map(str::to_string).to_vec();

            export_section.retain(|export| exports.contains(&export.name.to_string()));
        }
    }

    pub fn serialize(&self) -> Result<Vec<u8>> {
        self.module
            .serialize()
            .context("Failed to serialize module")
    }

    pub fn flush_to_file(self, path: &PathBuf) {
        fs::write(path, self.module.serialize().unwrap()).unwrap();
    }
}

pub struct OptimizationResult {
    pub original_size: f64,
    pub optimized_size: f64,
}

/// Attempts to perform optional Wasm optimization using `binaryen`.
///
/// The intention is to reduce the size of bloated Wasm binaries as a result of
/// missing optimizations (or bugs?) between Rust and Wasm.
pub fn optimize_wasm<P: AsRef<Path>>(
    source: P,
    destination: P,
    optimization_passes: &str,
    keep_debug_symbols: bool,
) -> Result<OptimizationResult> {
    let original_size = metadata(&source)?.len() as f64 / 1000.0;

    do_optimization(
        &source,
        &destination,
        optimization_passes,
        keep_debug_symbols,
    )?;

    let destination = destination.as_ref();
    if !destination.exists() {
        return Err(anyhow!(
            "Optimization failed, optimized wasm output file `{}` not found.",
            destination.display()
        ));
    }

    let optimized_size = metadata(destination)?.len() as f64 / 1000.0;

    Ok(OptimizationResult {
        original_size,
        optimized_size,
    })
}

/// Optimizes the Wasm supplied as `crate_metadata.dest_wasm` using
/// the `wasm-opt` binary.
///
/// The supplied `optimization_level` denotes the number of optimization passes,
/// resulting in potentially a lot of time spent optimizing.
///
/// If successful, the optimized Wasm is written to `dest_optimized`.
pub fn do_optimization<P: AsRef<Path>>(
    dest_wasm: P,
    dest_optimized: P,
    optimization_level: &str,
    keep_debug_symbols: bool,
) -> Result<()> {
    // check `wasm-opt` is installed
    let which = which::which("wasm-opt");
    if which.is_err() {
        return Err(anyhow!(
            "wasm-opt not found! Make sure the binary is in your PATH environment.\n\n\
            We use this tool to optimize the size of your program's Wasm binary.\n\n\
            wasm-opt is part of the binaryen package. You can find detailed\n\
            installation instructions on https://github.com/WebAssembly/binaryen#tools.\n\n\
            There are ready-to-install packages for many platforms:\n\
            * Debian/Ubuntu: apt-get install binaryen\n\
            * Homebrew: brew install binaryen\n\
            * Arch Linux: pacman -S binaryen\n\
            * Windows: binary releases at https://github.com/WebAssembly/binaryen/releases"
                .bright_yellow()
        ));
    }
    let wasm_opt_path = which
        .as_ref()
        .expect("we just checked if `which` returned an err; qed")
        .as_path();
    log::info!("Path to wasm-opt executable: {}", wasm_opt_path.display());

    log::info!(
        "Optimization level passed to wasm-opt: {optimization_level}"
    );
    let mut command = Command::new(wasm_opt_path);
    command
        .arg(dest_wasm.as_ref())
        .arg(format!("-O{optimization_level}"))
        .arg("-o")
        .arg(dest_optimized.as_ref())
        .arg("-mvp")
        .arg("--enable-sign-ext")
        .arg("--enable-mutable-globals")
        .arg("--limit-segments")
        .arg("--pass-arg=limit-segments@1024")
        // the memory in our module is imported, `wasm-opt` needs to be told that
        // the memory is initialized to zeroes, otherwise it won't run the
        // memory-packing pre-pass.
        .arg("--zero-filled-memory")
        .arg("--dae")
        .arg("--vacuum");
    if keep_debug_symbols {
        command.arg("-g");
    }
    log::info!("Invoking wasm-opt with {command:?}");
    let output = command.output().unwrap();

    if !output.status.success() {
        let err = std::str::from_utf8(&output.stderr)
            .expect("Cannot convert stderr output of wasm-opt to string")
            .trim();
        panic!(
            "The wasm-opt optimization failed.\n\n\
            The error which wasm-opt returned was: \n{err}"
        );
    }
    Ok(())
}
