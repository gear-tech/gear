// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Module for programs.

use crate::{code::InstrumentedCode, ids::ProgramId, memory::WasmPage};
use alloc::collections::BTreeSet;
use scale_info::scale::{Decode, Encode};

/// Program.
#[derive(Clone, Debug, Decode, Encode)]
pub struct Program {
    id: ProgramId,
    code: InstrumentedCode,
    /// Wasm pages allocated by program.
    allocations: BTreeSet<WasmPage>,
    /// Program is initialized.
    is_initialized: bool,
}

impl Program {
    /// New program with specific `id` and `code`.
    pub fn new(id: ProgramId, code: InstrumentedCode) -> Self {
        Program {
            id,
            code,
            allocations: Default::default(),
            is_initialized: false,
        }
    }

    /// New program from stored data
    pub fn from_parts(
        id: ProgramId,
        code: InstrumentedCode,
        allocations: BTreeSet<WasmPage>,
        is_initialized: bool,
    ) -> Self {
        Self {
            id,
            code,
            allocations,
            is_initialized,
        }
    }

    /// Reference to [`InstrumentedCode`] of this program.
    pub fn code(&self) -> &InstrumentedCode {
        &self.code
    }

    /// Reference to raw binary code of this program.
    pub fn raw_code(&self) -> &[u8] {
        self.code.code()
    }

    /// Get the [`ProgramId`] of this program.
    pub fn id(&self) -> ProgramId {
        self.id
    }

    /// Get initial memory size for this program.
    pub fn static_pages(&self) -> WasmPage {
        self.code.static_pages()
    }

    /// Get whether program is initialized
    ///
    /// By default the [`Program`] is not initialized. The initialized status
    /// is set from the node.
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    /// Set program initialized
    pub fn set_initialized(&mut self) {
        self.is_initialized = true;
    }

    /// Get allocations as a set of page numbers.
    pub fn allocations(&self) -> &BTreeSet<WasmPage> {
        &self.allocations
    }

    /// Set allocations as a set of page numbers.
    pub fn set_allocations(&mut self, allocations: BTreeSet<WasmPage>) {
        self.allocations = allocations;
    }
}

#[cfg(test)]
/// This module contains tests of `fn encode_hex(bytes: &[u8]) -> String`
/// and ProgramId's `fn from_slice(s: &[u8]) -> Self` constructor
mod tests {
    use super::Program;
    use crate::{code::Code, ids::ProgramId};
    use alloc::vec::Vec;
    use gear_wasm_instrument::wasm_instrument::gas_metering::ConstantCostRules;

    fn parse_wat(source: &str) -> Vec<u8> {
        let module_bytes = wabt::Wat2Wasm::new()
            .validate(false)
            .convert(source)
            .expect("failed to parse module")
            .as_ref()
            .to_vec();
        module_bytes
    }

    #[test]
    #[should_panic(expected = "Identifier must be 32 length")]
    /// Test that ProgramId's `from_slice(...)` constructor causes panic
    /// when the argument has the wrong length
    fn program_id_from_slice_error_implementation() {
        let bytes = "foobar";
        let _: ProgramId = bytes.as_bytes().into();
    }

    #[test]
    /// Test static pages.
    fn program_memory() {
        let wat = r#"
            (module
                (import "env" "gr_reply_to"  (func $gr_reply_to (param i32)))
                (import "env" "memory" (memory 2))
                (export "handle" (func $handle))
                (export "handle_reply" (func $handle))
                (export "init" (func $init))
                (func $handle
                    i32.const 65536
                    call $gr_reply_to
                )
                (func $handle_reply
                    i32.const 65536
                    call $gr_reply_to
                )
                (func $init)
            )"#;

        let binary: Vec<u8> = parse_wat(wat);

        let code = Code::try_new(binary, 1, |_| ConstantCostRules::default(), None).unwrap();
        let (code, _) = code.into_parts();
        let program = Program::new(ProgramId::from(1), code);

        // 2 static pages
        assert_eq!(program.static_pages(), 2.into());

        // Has no allocations because we do not set them in new
        assert_eq!(program.allocations().len(), 0);
    }
}
