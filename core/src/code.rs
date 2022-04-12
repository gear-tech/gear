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

//! Module for checked code.

use crate::ids::CodeId;
use alloc::vec::Vec;
use anyhow::Result;
use codec::{Decode, Encode};
use parity_wasm::elements::Module;
use wasm_instrument::gas_metering::Rules;

/// Instrumentation error.
#[derive(Debug)]
pub enum CodeError {
    /// Error occurred during checking original program code.
    ///
    /// The provided code doesn't contains needed imports or contains forbidden instructions.
    CheckError,
    /// Error occurred during decoding original program code.
    ///
    /// The provided code was a malformed Wasm bytecode or contained unsupported features
    /// (atomics, simd instructions, etc.).
    Decode,
    /// Error occurred during injecting gas metering instructions.
    ///
    /// This might be due to program contained unsupported/non-deterministic instructions
    /// (floats, manual memory grow, etc.).
    GasInjection,
    /// Error occurred during encoding instrumented program.
    ///
    /// The only possible reason for that might be OOM.
    Encode,
}

use crate::memory::WasmPageNumber;

/// Contains instrumented binary code of a program and initial memory size from memory import.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct Code {
    /// Code instrumented with the latest schedule.
    code: Vec<u8>,
    /// The uninstrumented, original version of the code.
    ///
    /// It is not stored because the original code has its own storage item. The value
    /// is only `Some` when this module was created from an `original_code` and `None` if
    /// it was loaded from storage.
    #[codec(skip)]
    original_code: Option<Vec<u8>>,
    /// The code hash of the stored code which is defined as the hash over the `original_code`.
    ///
    /// As the map key there is no need to store the hash in the value, too. It is set manually
    /// when loading the code from storage.
    #[codec(skip)]
    code_hash: CodeId,
    static_pages: WasmPageNumber,
    #[codec(compact)]
    instruction_weights_version: u32,
}

impl Code {
    /// Create the code by checking and instrumenting `original_code`.
    pub fn try_new(
        original_code: Vec<u8>,
        version: u32,
        module: Option<Module>,
        gas_rules: impl Rules,
    ) -> Result<Self, CodeError> {
        let module = module.unwrap_or(
            wasm_instrument::parity_wasm::deserialize_buffer(&original_code)
                .map_err(|_| CodeError::Decode)?,
        );

        // get initial memory size from memory import.
        let static_pages = WasmPageNumber(
            module
                .import_section()
                .ok_or(CodeError::CheckError)?
                .entries()
                .iter()
                .find_map(|entry| match entry.external() {
                    parity_wasm::elements::External::Memory(mem_ty) => {
                        Some(mem_ty.limits().initial())
                    }
                    _ => None,
                })
                .ok_or(CodeError::CheckError)?,
        );

        let instrumented_module = wasm_instrument::gas_metering::inject(module, &gas_rules, "env")
            .map_err(|_| CodeError::GasInjection)?;

        let instrumented = wasm_instrument::parity_wasm::elements::serialize(instrumented_module)
            .map_err(|_| CodeError::Encode)?;

        let code_hash = CodeId::generate(&original_code);

        Ok(Self {
            original_code: Some(original_code),
            code: instrumented,
            static_pages,
            instruction_weights_version: version,
            code_hash,
        })
    }

    /// Returns the original code.
    pub fn original_code(&self) -> Option<&Vec<u8>> {
        self.original_code.as_ref()
    }

    /// Returns reference to the instrumented binary code.
    pub fn code(&self) -> &[u8] {
        &self.code
    }

    /// Returns code hash.
    pub fn code_hash(&self) -> CodeId {
        self.code_hash
    }

    /// Set code hash.
    pub fn set_code_hash(&mut self, code_hash: CodeId) {
        self.code_hash = code_hash;
    }

    /// Returns instruction weights version.
    pub fn instruction_weights_version(&self) -> u32 {
        self.instruction_weights_version
    }

    /// Returns initial memory size from memory import.
    pub fn static_pages(&self) -> WasmPageNumber {
        self.static_pages
    }
}
