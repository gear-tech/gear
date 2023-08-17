// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! Wasm generator that can generate valid gear wasm programs.
//!
//! By gear wasm program we mean wasm modules that can be stored
//! and executed on the [gear](https://www.gear-tech.io/) runtime.
//!
//! This crate also re-exports `arbitrary` from internal module [`wasm_gen_arbitrary`] to make it easy generating arbitrary configs and wasm.

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
pub use gear_wasm_instrument::syscalls::SysCallName;
pub use generator::*;
pub use wasm::WasmModule;
pub use wasm_gen_arbitrary::*;

use gear_wasm_instrument::parity_wasm::{self, elements::Module};

/// Generate gear program as raw bytes.
pub fn generate_gear_program_code(
    u: &mut Unstructured<'_>,
    configs_bundle: impl ConfigsBundle,
) -> Result<Vec<u8>> {
    let module = generate_gear_program_module(u, configs_bundle)?;

    let bytes = parity_wasm::serialize(module).expect("unable to serialize pw module");

    log::trace!(
        "{}",
        wasmprinter::print_bytes(&bytes).expect("internal error: failed printing bytes")
    );

    Ok(bytes)
}

/// Generate gear program as [`parity_wasm::elements::Module`](https://docs.rs/parity-wasm/latest/parity_wasm/elements/struct.Module.html)
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
