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

use crate::{ids::CodeId, memory::WasmPageNumber, message::DispatchKind};
use alloc::{collections::BTreeSet, vec::Vec};
use codec::{Decode, Encode};
use parity_wasm::elements::{Internal, Module};
use scale_info::TypeInfo;
use wasm_instrument::gas_metering::Rules;

/// Defines maximal permitted count of memory pages.
pub const MAX_WASM_PAGE_COUNT: u32 = 512;

/// Parse function exports from wasm module into [`DispatchKind`].
fn get_exports(
    module: &Module,
    reject_unnececery: bool,
) -> Result<BTreeSet<DispatchKind>, CodeError> {
    let mut exports = BTreeSet::<DispatchKind>::new();

    for entry in module
        .export_section()
        .ok_or(CodeError::ExportSectionNotFound)?
        .entries()
        .iter()
    {
        if let Internal::Function(_) = entry.internal() {
            if entry.field() == DispatchKind::Init.into_entry() {
                exports.insert(DispatchKind::Init);
            } else if entry.field() == DispatchKind::Handle.into_entry() {
                exports.insert(DispatchKind::Handle);
            } else if entry.field() == DispatchKind::Reply.into_entry() {
                exports.insert(DispatchKind::Reply);
            } else if reject_unnececery {
                return Err(CodeError::NonGearExportFnFound);
            }
        }
    }
    Ok(exports)
}

/// Instrumentation error.
#[derive(Debug)]
pub enum CodeError {
    /// The provided code doesn't contain required import section.
    ImportSectionNotFound,
    /// The provided code doesn't contain memory entry section.
    MemoryEntryNotFound,
    /// The provided code doesn't contain export section.
    ExportSectionNotFound,
    /// The provided code doesn't contain the required `init` or `handle` export function.
    RequiredExportFnNotFound,
    /// The provided code contains unnecessary function exports.
    NonGearExportFnFound,
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
    /// We restrict start sections in smart contracts.
    StartSectionExists,
    /// We restrict custom sections in smart contracts.
    CustomSectionsExist,
    /// The provided code has invalid count of static pages.
    InvalidStaticPageCount,
}

/// Contains instrumented binary code of a program and initial memory size from memory import.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
pub struct Code {
    /// Code instrumented with the latest schedule.
    code: Vec<u8>,
    /// The uninstrumented, original version of the code.
    raw_code: Vec<u8>,
    /// Exports of the wasm module.
    exports: BTreeSet<DispatchKind>,
    static_pages: WasmPageNumber,
    #[codec(compact)]
    instruction_weights_version: u32,
}

impl Code {
    /// Create the code by checking and instrumenting `original_code`.
    pub fn try_new<R, GetRulesFn>(
        raw_code: Vec<u8>,
        version: u32,
        mut get_gas_rules: GetRulesFn,
    ) -> Result<Self, CodeError>
    where
        R: Rules,
        GetRulesFn: FnMut(&Module) -> R,
    {
        let module: Module = wasm_instrument::parity_wasm::deserialize_buffer(&raw_code)
            .map_err(|_| CodeError::Decode)?;

        if module.start_section().is_some() {
            log::debug!("Found start section in contract code, which is not allowed");
            return Err(CodeError::StartSectionExists);
        }

        if module.custom_sections().count() != 0 {
            log::debug!("Found custom sections in contract code, which is not allowed");
            return Err(CodeError::CustomSectionsExist);
        }

        // get initial memory size from memory import.
        let static_pages = WasmPageNumber(
            module
                .import_section()
                .ok_or(CodeError::ImportSectionNotFound)?
                .entries()
                .iter()
                .find_map(|entry| match entry.external() {
                    parity_wasm::elements::External::Memory(mem_ty) => {
                        Some(mem_ty.limits().initial())
                    }
                    _ => None,
                })
                .ok_or(CodeError::MemoryEntryNotFound)?,
        );

        if static_pages > MAX_WASM_PAGE_COUNT.into() {
            return Err(CodeError::InvalidStaticPageCount);
        }

        let exports = get_exports(&module, true)?;

        if exports.contains(&DispatchKind::Init) || exports.contains(&DispatchKind::Handle) {
            let gas_rules = get_gas_rules(&module);
            let instrumented_module =
                wasm_instrument::gas_metering::inject(module, &gas_rules, "env")
                    .map_err(|_| CodeError::GasInjection)?;

            let instrumented =
                wasm_instrument::parity_wasm::elements::serialize(instrumented_module)
                    .map_err(|_| CodeError::Encode)?;

            Ok(Self {
                code: instrumented,
                raw_code,
                exports,
                static_pages,
                instruction_weights_version: version,
            })
        } else {
            Err(CodeError::RequiredExportFnNotFound)
        }
    }

    /// Create the code without checks.
    pub fn new_raw(
        original_code: Vec<u8>,
        version: u32,
        module: Option<Module>,
        instrument_with_const_rules: bool,
    ) -> Result<Self, CodeError> {
        let module = module.unwrap_or(
            wasm_instrument::parity_wasm::deserialize_buffer(&original_code)
                .map_err(|_| CodeError::Decode)?,
        );

        // get initial memory size from memory import.
        let static_pages = WasmPageNumber(
            module
                .import_section()
                .ok_or(CodeError::ImportSectionNotFound)?
                .entries()
                .iter()
                .find_map(|entry| match entry.external() {
                    parity_wasm::elements::External::Memory(mem_ty) => {
                        Some(mem_ty.limits().initial())
                    }
                    _ => None,
                })
                .ok_or(CodeError::MemoryEntryNotFound)?,
        );

        let exports = get_exports(&module, false)?;

        if exports.contains(&DispatchKind::Init) || exports.contains(&DispatchKind::Handle) {
            if instrument_with_const_rules {
                let instrumented_module = wasm_instrument::gas_metering::inject(
                    module,
                    &wasm_instrument::gas_metering::ConstantCostRules::default(),
                    "env",
                )
                .map_err(|_| CodeError::GasInjection)?;

                let instrumented =
                    wasm_instrument::parity_wasm::elements::serialize(instrumented_module)
                        .map_err(|_| CodeError::Encode)?;

                Ok(Self {
                    raw_code: original_code,
                    code: instrumented,
                    exports,
                    static_pages,
                    instruction_weights_version: version,
                })
            } else {
                Ok(Self {
                    raw_code: original_code.clone(),
                    code: original_code,
                    exports,
                    static_pages,
                    instruction_weights_version: version,
                })
            }
        } else {
            Err(CodeError::RequiredExportFnNotFound)
        }
    }

    /// Returns the original code.
    pub fn raw_code(&self) -> &[u8] {
        &self.raw_code
    }

    /// Returns reference to the instrumented binary code.
    pub fn code(&self) -> &[u8] {
        &self.code
    }

    /// Returns wasm module exports.
    pub fn exports(&self) -> &BTreeSet<DispatchKind> {
        &self.exports
    }

    /// Returns instruction weights version.
    pub fn instruction_weights_version(&self) -> u32 {
        self.instruction_weights_version
    }

    /// Returns initial memory size from memory import.
    pub fn static_pages(&self) -> WasmPageNumber {
        self.static_pages
    }

    /// Consumes this instance and returns the instrumented and raw binary codes.
    pub fn into_parts(self) -> (InstrumentedCode, Vec<u8>) {
        (
            InstrumentedCode {
                code: self.code,
                exports: self.exports,
                static_pages: self.static_pages,
                version: self.instruction_weights_version,
            },
            self.raw_code,
        )
    }
}

/// The newtype contains the Code instance and the corresponding id (hash).
#[derive(Clone, Debug)]
pub struct CodeAndId {
    code: Code,
    code_id: CodeId,
}

impl CodeAndId {
    /// Calculates the id (hash) of the raw binary code and creates new instance.
    pub fn new(code: Code) -> Self {
        let code_id = CodeId::generate(code.raw_code());
        Self { code, code_id }
    }

    /// Creates the instance from the precalculated hash without checks.
    pub fn from_parts_unchecked(code: Code, code_id: CodeId) -> Self {
        debug_assert_eq!(code_id, CodeId::generate(code.raw_code()));
        Self { code, code_id }
    }

    /// Returns corresponding id (hash) for the code.
    pub fn code_id(&self) -> CodeId {
        self.code_id
    }

    /// Returns reference to Code.
    pub fn code(&self) -> &Code {
        &self.code
    }

    /// Decomposes this instance.
    pub fn into_parts(self) -> (Code, CodeId) {
        (self.code, self.code_id)
    }
}

/// The newtype contains the instrumented code and the corresponding id (hash).
#[derive(Clone, Debug, Decode, Encode, TypeInfo)]
pub struct InstrumentedCode {
    code: Vec<u8>,
    exports: BTreeSet<DispatchKind>,
    static_pages: WasmPageNumber,
    version: u32,
}

impl InstrumentedCode {
    /// Returns reference to the instrumented binary code.
    pub fn code(&self) -> &[u8] {
        &self.code
    }

    /// Returns instruction weights version.
    pub fn instruction_weights_version(&self) -> u32 {
        self.version
    }

    /// Returns wasm module exports.
    pub fn exports(&self) -> &BTreeSet<DispatchKind> {
        &self.exports
    }

    /// Returns initial memory size from memory import.
    pub fn static_pages(&self) -> WasmPageNumber {
        self.static_pages
    }

    /// Consumes the instance and returns the instrumented code.
    pub fn into_code(self) -> Vec<u8> {
        self.code
    }
}

/// The newtype contains the instrumented code and the corresponding id (hash).
#[derive(Clone, Debug, Decode, Encode)]
pub struct InstrumentedCodeAndId {
    code: InstrumentedCode,
    code_id: CodeId,
}

impl InstrumentedCodeAndId {
    /// Returns reference to the instrumented code.
    pub fn code(&self) -> &InstrumentedCode {
        &self.code
    }

    /// Returns corresponding id (hash) for the code.
    pub fn code_id(&self) -> CodeId {
        self.code_id
    }

    /// Consumes the instance and returns the instrumented code.
    pub fn into_parts(self) -> (InstrumentedCode, CodeId) {
        (self.code, self.code_id)
    }
}

impl From<CodeAndId> for InstrumentedCodeAndId {
    fn from(code_and_id: CodeAndId) -> Self {
        let (code, code_id) = code_and_id.into_parts();
        let (code, _) = code.into_parts();
        Self { code, code_id }
    }
}
