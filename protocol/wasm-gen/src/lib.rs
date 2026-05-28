// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Wasm generator that can generate valid gear wasm programs.
//!
//! By gear wasm program we mean wasm modules that can be stored
//! and executed on the [gear](https://www.gear-tech.io/) runtime.
//!
//! This crate also re-exports `arbitrary` from internal module [`wasm_gen_arbitrary`] to make it easy generating arbitrary configs and wasm.

#![allow(clippy::items_after_test_module)]
pub mod wasm_gen_arbitrary {
    //! `arbitrary` crate re-export.
    pub use arbitrary::*;
}
pub mod config;
pub mod generator;

#[cfg(test)]
mod tests;
mod utils;
mod wasm;

pub use config::*;
pub use gear_wasm_instrument::syscalls::{SyscallKind, SyscallName};
pub use generator::*;
pub use wasm::{MemoryLayout, WasmModule};
pub use wasm_gen_arbitrary::*;

use gear_wasm_instrument::Module;

/// Generate gear program as raw bytes.
pub fn generate_gear_program_code(
    u: &mut Unstructured<'_>,
    configs_bundle: impl ConfigsBundle,
) -> Result<Vec<u8>> {
    let module = generate_gear_program_module(u, configs_bundle)?;

    let bytes = module.serialize().expect("unable to serialize pw module");

    log::trace!(
        "{}",
        wasmprinter::print_bytes(&bytes).expect("internal error: failed printing bytes")
    );

    Ok(bytes)
}

/// Generate gear program as [`gear_wasm_instrument::Module`]
pub fn generate_gear_program_module(
    u: &mut Unstructured<'_>,
    configs_bundle: impl ConfigsBundle,
) -> Result<Module> {
    let (gear_wasm_generator_config, module_selectables_config) = configs_bundle.into_parts();

    let arbitrary_params = u.arbitrary::<ArbitraryParams>()?;
    let wasm_module =
        WasmModule::generate_with_config((module_selectables_config, arbitrary_params).into(), u)?;

    GearWasmGenerator::new_with_config(wasm_module, u, gear_wasm_generator_config).generate()
}
