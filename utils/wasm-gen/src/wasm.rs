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

use crate::{config::WasmModuleConfig, EntryPointName};
use arbitrary::{Arbitrary, Result, Unstructured};
use core::mem;
use gear_wasm_instrument::parity_wasm::{
    self,
    elements::{External, Internal, Module},
};
use wasm_smith::Module as WasmSmithModule;

/// Wasm module.
///
/// Actually that's a wrapper over `parity-wasm::elements::Module`,
/// that functions as an adaptor for it by exposing a higher level API
/// of a wasm module.
pub struct WasmModule(Module);

impl WasmModule {
    /// Same as [`WasmModule::generate_with_config`], but generates an arbitrary config
    /// instead of using the external one.
    pub fn generate(u: &mut Unstructured<'_>) -> Result<Self> {
        let config = WasmModuleConfig::arbitrary(u)?;

        Self::generate_with_config(config, u)
    }

    /// Generate a random wasm module from `Unstructured`.
    ///
    /// Under the hood it uses the `wasm-smith` wasm generator to generate a new valid wasm
    /// out of random bytes provider.
    ///
    /// If generated module hasn't got functions section, i.e. no internal functions were generated,
    /// than this function will return an error.
    pub fn generate_with_config(
        config: WasmModuleConfig,
        u: &mut Unstructured<'_>,
    ) -> Result<Self> {
        let mut pw_module = Self::generate_wasm_smith_module(config.clone(), u)?;
        while pw_module.function_section().is_none() {
            pw_module = Self::generate_wasm_smith_module(config.clone(), u)?;
        }

        Ok(Self(pw_module))
    }

    /// Counts functions in import section.
    pub fn count_import_funcs(&self) -> usize {
        self.0.import_section().map_or(0, |isec| isec.functions())
    }

    /// Counts functions in function section.
    pub fn count_code_funcs(&self) -> usize {
        self.0
            .function_section()
            .map(|fsec| fsec.entries().len())
            .expect("minimal possible is 1 by config")
    }

    /// Returns an option with a value of initial memory size,
    /// defined in the import section.
    ///
    /// This is also referred sometime as "min" memory limit.
    pub fn initial_mem_size(&self) -> Option<u32> {
        self.0.import_section().and_then(|import_entry| {
            import_entry
                .entries()
                .iter()
                .find_map(|entry| match entry.external() {
                    External::Memory(mem_ty) => Some(mem_ty.limits().initial()),
                    _ => None,
                })
        })
    }

    /// Gets the export function index of the gear entry point.
    pub fn gear_entry_point(&self, ep: EntryPointName) -> Option<u32> {
        self.0.export_section().and_then(|export_section| {
            for export in export_section.entries().iter() {
                if export.field() == ep.to_str() {
                    let &Internal::Function(init_idx) = export.internal() else {
                            panic!("init export is not a func");
                        };
                    return Some(init_idx);
                }
            }

            None
        })
    }

    /// Executes some job `f` on the underlying module.
    ///
    /// This method is used as a guard access to the underlying module.
    ///
    /// There's a contract, that the `f` must return the same module, which,
    /// possibly, could have been changed, as a first element of the tuple.
    /// The second element of the tuple, `T`,  is the type returned to the caller.
    pub fn with<T>(&mut self, f: impl FnOnce(Module) -> (Module, T)) -> T {
        let taken_module = mem::take(&mut self.0);
        let (mut res_module, res) = f(taken_module);
        mem::swap(&mut self.0, &mut res_module);

        res
    }

    /// Unwraps the underlying wasm module.
    pub fn into_inner(self) -> Module {
        self.0
    }

    fn generate_wasm_smith_module(
        config: WasmModuleConfig,
        u: &mut Unstructured<'_>,
    ) -> Result<Module> {
        let wasm_smith_module = WasmSmithModule::new(config.into_inner(), u)?;
        Ok(
            parity_wasm::deserialize_buffer(wasm_smith_module.to_bytes().as_ref())
                .expect("internal error: wasm smith generated non-deserializable module"),
        )
    }
}

/// WASM page has size of 64KiBs (65_536 bytes)
pub(crate) const PAGE_SIZE: u32 = 0x10000;

/// Struct for indexing WASM memory page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Page(u16);

impl From<u16> for Page {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

/// New-type to represent WASM memory pages count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageCount(u32);

impl From<u32> for PageCount {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl PageCount {
    /// Calculate WASM memory size for this pages count.
    pub(crate) fn memory_size(&self) -> u32 {
        self.0 * PAGE_SIZE
    }

    /// Get WASM memory pages count as a number.
    pub(crate) fn raw(&self) -> u32 {
        self.0
    }
}
