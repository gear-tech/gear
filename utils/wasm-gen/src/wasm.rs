// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Wasm related entities.

use crate::{EntryPointName, config::WasmModuleConfig};
use arbitrary::{Result, Unstructured};
use core::mem;
use gear_core::pages::WasmPage;
use gear_wasm_instrument::{Instruction, Module, STACK_END_EXPORT_NAME};
use gsys::{Handle, Hash};
use wasm_smith::Module as WasmSmithModule;
use wasmparser::{ExternalKind, TypeRef};

/// Wasm module.
///
/// Actually that's a wrapper over `gear_wasm_instrument::Module`,
/// that functions as an adaptor for it by exposing a higher level API
/// of a wasm module.
pub struct WasmModule(Module);

impl WasmModule {
    /// Generate a random wasm module from `Unstructured`.
    ///
    /// Under the hood it uses the `wasm-smith` wasm generator to generate a new valid wasm
    /// out of random bytes provider.
    ///
    /// If generated module hasn't got functions section, i.e. no internal functions were generated,
    /// than this function will return an error.
    pub fn generate_with_config(config: WasmModuleConfig, u: &mut Unstructured) -> Result<Self> {
        let wasm_smith_module = WasmSmithModule::new(config.into_inner(), u)?.to_bytes();

        let module = Module::new(&wasm_smith_module)
            .expect("internal error: wasm smith generated non-deserializable module");
        if module.function_section.is_none() {
            panic!(
                "WasmModule::generate_with_config: `wasm-smith` config doesn't guarantee having function section!"
            );
        }

        Ok(WasmModule(module))
    }

    /// Counts functions in import section.
    pub fn count_import_funcs(&self) -> usize {
        self.0.import_section.as_ref().map_or(0, |isec| {
            isec.iter()
                .filter(|import| matches!(import.ty, TypeRef::Func(_)))
                .count()
        })
    }

    /// Counts functions in function section.
    pub fn count_code_funcs(&self) -> usize {
        self.0
            .function_section
            .as_ref()
            .map(|fsec| fsec.len())
            .expect("minimal possible is 1 by config")
    }

    /// Counts amount of instructions in the provided function.
    pub fn count_func_instructions(&self, func_id: usize) -> usize {
        self.0
            .code_section
            .as_ref()
            .expect("has at least one function by config")
            .get(func_id)
            .expect("invalid `func_id`")
            .instructions
            .len()
    }

    /// Returns an option with a value of initial memory size,
    /// defined in the import section.
    ///
    /// This is also referred sometime as "min" memory limit.
    pub fn initial_mem_size(&self) -> Option<u32> {
        self.0.import_section.as_ref().and_then(|import_entry| {
            import_entry.iter().find_map(|entry| match entry.ty {
                TypeRef::Memory(mem_ty) => Some(mem_ty.initial as u32),
                _ => None,
            })
        })
    }

    pub fn get_stack_end_offset(&self) -> Option<i32> {
        let stack_end_global_index = self
            .0
            .export_section
            .as_ref()?
            .iter()
            .find(|export| export.name == STACK_END_EXPORT_NAME)
            .and_then(|export_entry| match export_entry.kind {
                ExternalKind::Global => Some(export_entry.index),
                _ => None,
            })?;

        let stack_end_init_expr = self
            .0
            .global_section
            .as_ref()?
            .get(stack_end_global_index as usize)?
            .init_expr
            .instructions
            .as_slice();

        match stack_end_init_expr {
            [Instruction::I32Const(value)] => Some(*value),
            _ => None,
        }
    }

    /// Gets the export function index of the gear entry point.
    pub fn gear_entry_point(&self, ep: EntryPointName) -> Option<u32> {
        self.0.export_section.as_ref().and_then(|export_section| {
            for export in export_section.iter() {
                if export.name == ep.to_str() {
                    let ExternalKind::Func = export.kind else {
                        panic!("init export is not a func");
                    };
                    return Some(export.index);
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
}

/// New-type to represent WASM memory pages count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PageCount(u32);

impl From<u32> for PageCount {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl PageCount {
    /// Calculate WASM memory size for this pages count.
    pub(crate) fn memory_size(&self) -> u32 {
        self.0 * WasmPage::SIZE
    }
}

macro_rules! def_memory_layout {
    (
        $(
            $(#[$attr:meta])*
            struct $name:ident {
                $(
                    $field:ident: $ftype:ty
                ),* $(,)?
            }
        )*
    ) => {
        $(
            $(#[$attr])*
            pub struct $name {
                $(
                    pub $field: i32,
                )*
            }

            impl From<u32> for $name {
                fn from(mem_size: u32) -> Self {
                    #[repr(C, packed)]
                    struct WasmMemoryLayout {
                        $(
                            $field: $ftype,
                        )*
                    }

                    const {
                        assert!(
                            size_of::<WasmMemoryLayout>() as u32 <= $name::RESERVED_MEMORY_SIZE,
                            "reserved memory exceeded"
                        )
                    };

                    let start_memory_ptr = mem_size.saturating_sub($name::RESERVED_MEMORY_SIZE) as i32;

                    $(
                        let $field = start_memory_ptr + mem::offset_of!(WasmMemoryLayout, $field) as i32;
                    )*

                    Self {
                        $(
                            $field,
                        )*
                    }
                }
            }
        )*
    };
}

def_memory_layout! {
    /// Represents memory layout that can be safely used between syscalls and
    /// instructions.
    ///
    /// The last memory page in program generated by `wasm-gen` is reserved for
    /// internal use. Currently, we take [`MemoryLayout::RESERVED_MEMORY_SIZE`]
    /// bytes from the last memory page and also prohibit modification of this
    /// memory at the `wasm-smith` and `wasm-gen` level.
    ///
    /// If you want to store some data in memory and then access it in the program,
    /// consider adding a new pointer to this structure.
    struct MemoryLayout {
        init_called_ptr: bool,
        wait_called_ptr: u32,
        handle_temp1_ptr: u32,
        handle_temp2_ptr: u32,
        handle_flags_ptr: u32,
        handle_array_ptr: [Handle; MemoryLayout::AMOUNT_OF_HANDLES as _],
        reservation_temp1_ptr: u32,
        reservation_temp2_ptr: u32,
        reservation_flags_ptr: u32,
        reservation_array_ptr: [Hash; MemoryLayout::AMOUNT_OF_RESERVATIONS as _],
        waited_message_id_ptr: Hash,
    }
}

impl MemoryLayout {
    /// The amount of reserved memory.
    pub const RESERVED_MEMORY_SIZE: u32 = 256;

    /// The amount of handles.
    pub const AMOUNT_OF_HANDLES: u32 = 5;
    /// The amount of reservation ids.
    pub const AMOUNT_OF_RESERVATIONS: u32 = 5;
}
