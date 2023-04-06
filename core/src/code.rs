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

use crate::{
    ids::CodeId,
    memory::{PageU32Size, WasmPage},
    message::{DispatchKind, WasmEntry},
};
use alloc::{collections::BTreeSet, vec::Vec};
use core::ops::ControlFlow;
use gear_wasm_instrument::{
    parity_wasm::{
        self,
        elements::{Instruction, Internal, Module},
    },
    wasm_instrument::{
        self,
        gas_metering::{ConstantCostRules, Rules},
    },
    STACK_END_EXPORT_NAME,
};
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

/// Defines maximal permitted count of memory pages.
pub const MAX_WASM_PAGE_COUNT: u16 = 512;

/// Name of exports allowed on chain except execution kinds.
pub const STATE_EXPORTS: [&str; 2] = ["state", "metahash"];

/// Parse function exports from wasm module into [`DispatchKind`].
fn get_exports(
    module: &Module,
    reject_unnecessary: bool,
) -> Result<BTreeSet<DispatchKind>, CodeError> {
    let mut exports = BTreeSet::<DispatchKind>::new();

    for entry in module
        .export_section()
        .ok_or(CodeError::ExportSectionNotFound)?
        .entries()
        .iter()
    {
        if let Internal::Function(_) = entry.internal() {
            if let Some(kind) = DispatchKind::try_from_entry(entry.field()) {
                exports.insert(kind);
            } else if !STATE_EXPORTS.contains(&entry.field()) && reject_unnecessary {
                return Err(CodeError::NonGearExportFnFound);
            }
        }
    }

    Ok(exports)
}

fn get_stack_end_init_code(module: &Module) -> Option<&[Instruction]> {
    let global_index = module
        .export_section()?
        .entries()
        .iter()
        .try_for_each(|entry| match entry.internal() {
            Internal::Global(index) if entry.field() == STACK_END_EXPORT_NAME => {
                ControlFlow::Break(*index)
            }
            _ => ControlFlow::Continue(()),
        });

    let ControlFlow::Break(global_index) = global_index else {
        return None;
    };

    let section = module.global_section()?;
    let entry = &section.entries()[global_index as usize];

    Some(entry.init_expr().code())
}

fn get_offset_i32(init_code: &[Instruction]) -> Option<u32> {
    use Instruction::{End, I32Const};

    if init_code.len() != 2 {
        return None;
    }

    match (&init_code[0], &init_code[1]) {
        (I32Const(stack_end), End) => Some(*stack_end as u32),
        _ => None,
    }
}

fn check_gear_stack_end(module: &Module) -> Result<(), CodeError> {
    let Some(init_expr) = get_stack_end_init_code(module) else {
        return Ok(());
    };

    let stack_end = get_offset_i32(init_expr).ok_or(CodeError::StackEndInitialization)?;
    let Some(section) = module.data_section() else {
        return Ok(());
    };

    for data_segment in section.entries() {
        let offset = data_segment
            .offset()
            .as_ref()
            .and_then(|init_expr| get_offset_i32(init_expr.code()))
            .ok_or(CodeError::DataSegmentInitialization)?;

        if offset < stack_end {
            return Err(CodeError::StackEndOverlaps);
        }
    }

    Ok(())
}

/// Instrumentation error.
#[derive(Debug, PartialEq, Eq)]
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
    /// Error occurred during stack height instrumentation.
    StackLimitInjection,
    /// Error occurred during encoding instrumented program.
    ///
    /// The only possible reason for that might be OOM.
    Encode,
    /// We restrict start sections in smart contracts.
    StartSectionExists,
    /// The provided code has invalid count of static pages.
    InvalidStaticPageCount,
    /// Unsupported initialization of gear stack end global variable.
    StackEndInitialization,
    /// Unsupported initialization of data segment.
    DataSegmentInitialization,
    /// Pointer to the stack end overlaps data segment.
    StackEndOverlaps,
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
    static_pages: WasmPage,
    #[codec(compact)]
    instruction_weights_version: u32,
}

impl Code {
    /// Create the code by checking and instrumenting `original_code`.
    pub fn try_new<R, GetRulesFn>(
        raw_code: Vec<u8>,
        version: u32,
        mut get_gas_rules: GetRulesFn,
        stack_height: Option<u32>,
    ) -> Result<Self, CodeError>
    where
        R: Rules,
        GetRulesFn: FnMut(&Module) -> R,
    {
        wasmparser::validate(&raw_code).map_err(|_| CodeError::Decode)?;

        let module: Module =
            parity_wasm::deserialize_buffer(&raw_code).map_err(|_| CodeError::Decode)?;

        if module.start_section().is_some() {
            log::debug!("Found start section in contract code, which is not allowed");
            return Err(CodeError::StartSectionExists);
        }

        // get initial memory size from memory import.
        let static_pages_raw = module
            .import_section()
            .ok_or(CodeError::ImportSectionNotFound)?
            .entries()
            .iter()
            .find_map(|entry| match entry.external() {
                parity_wasm::elements::External::Memory(mem_ty) => Some(mem_ty.limits().initial()),
                _ => None,
            })
            .ok_or(CodeError::MemoryEntryNotFound)?;
        let static_pages =
            WasmPage::new(static_pages_raw).map_err(|_| CodeError::InvalidStaticPageCount)?;

        if static_pages.raw() > MAX_WASM_PAGE_COUNT as u32 {
            return Err(CodeError::InvalidStaticPageCount);
        }

        check_gear_stack_end(&module)?;

        let exports = get_exports(&module, true)?;

        if exports.contains(&DispatchKind::Init) || exports.contains(&DispatchKind::Handle) {
            let gas_rules = get_gas_rules(&module);
            let instrumented_module = gear_wasm_instrument::inject(module, &gas_rules, "env")
                .map_err(|_| CodeError::GasInjection)?;

            let instrumented = if let Some(limit) = stack_height {
                let instrumented_module =
                    wasm_instrument::inject_stack_limiter(instrumented_module, limit)
                        .map_err(|_| CodeError::StackLimitInjection)?;
                parity_wasm::elements::serialize(instrumented_module)
                    .map_err(|_| CodeError::Encode)?
            } else {
                parity_wasm::elements::serialize(instrumented_module)
                    .map_err(|_| CodeError::Encode)?
            };

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

    /// Create the code without instrumentation or instrumented
    /// with `ConstantCostRules`. There is also no check for static memory pages.
    pub fn new_raw(
        original_code: Vec<u8>,
        version: u32,
        module: Option<Module>,
        instrument_with_const_rules: bool,
        check_entries: bool,
    ) -> Result<Self, CodeError> {
        wasmparser::validate(&original_code).map_err(|_| CodeError::Decode)?;

        let module = module.unwrap_or(
            parity_wasm::deserialize_buffer(&original_code).map_err(|_| CodeError::Decode)?,
        );

        if module.start_section().is_some() {
            log::debug!("Found start section in contract code, which is not allowed");
            return Err(CodeError::StartSectionExists);
        }

        // get initial memory size from memory import.
        let static_pages = WasmPage::new(
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
        )
        .map_err(|_| CodeError::InvalidStaticPageCount)?;

        let exports = get_exports(&module, false)?;

        if check_entries
            && !(exports.contains(&DispatchKind::Init) || exports.contains(&DispatchKind::Handle))
        {
            return Err(CodeError::RequiredExportFnNotFound);
        }

        let code = if instrument_with_const_rules {
            let instrumented_module =
                gear_wasm_instrument::inject(module, &ConstantCostRules::default(), "env")
                    .map_err(|_| CodeError::GasInjection)?;

            parity_wasm::elements::serialize(instrumented_module).map_err(|_| CodeError::Encode)?
        } else {
            original_code.clone()
        };

        Ok(Self {
            raw_code: original_code,
            code,
            exports,
            static_pages,
            instruction_weights_version: version,
        })
    }

    /// Create the code with instrumentation, but without checks.
    /// There is also no check for static memory pages.
    pub fn new_raw_with_rules<R, GetRulesFn>(
        original_code: Vec<u8>,
        version: u32,
        check_entries: bool,
        mut get_gas_rules: GetRulesFn,
    ) -> Result<Self, CodeError>
    where
        R: Rules,
        GetRulesFn: FnMut(&Module) -> R,
    {
        wasmparser::validate(&original_code).map_err(|_| CodeError::Decode)?;

        let module: Module =
            parity_wasm::deserialize_buffer(&original_code).map_err(|_| CodeError::Decode)?;

        if module.start_section().is_some() {
            log::debug!("Found start section in contract code, which is not allowed");
            return Err(CodeError::StartSectionExists);
        }

        // get initial memory size from memory import.
        let static_pages_raw = module
            .import_section()
            .ok_or(CodeError::ImportSectionNotFound)?
            .entries()
            .iter()
            .find_map(|entry| match entry.external() {
                parity_wasm::elements::External::Memory(mem_ty) => Some(mem_ty.limits().initial()),
                _ => None,
            })
            .ok_or(CodeError::MemoryEntryNotFound)?;

        let static_pages =
            WasmPage::new(static_pages_raw).map_err(|_| CodeError::InvalidStaticPageCount)?;

        let exports = get_exports(&module, false)?;

        if check_entries
            && !(exports.contains(&DispatchKind::Init) || exports.contains(&DispatchKind::Handle))
        {
            return Err(CodeError::RequiredExportFnNotFound);
        }

        let gas_rules = get_gas_rules(&module);
        let instrumented_module = gear_wasm_instrument::inject(module, &gas_rules, "env")
            .map_err(|_| CodeError::GasInjection)?;

        let instrumented =
            parity_wasm::elements::serialize(instrumented_module).map_err(|_| CodeError::Encode)?;

        Ok(Self {
            raw_code: original_code,
            code: instrumented,
            exports,
            static_pages,
            instruction_weights_version: version,
        })
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
    pub fn static_pages(&self) -> WasmPage {
        self.static_pages
    }

    /// Consumes this instance and returns the instrumented and raw binary codes.
    pub fn into_parts(self) -> (InstrumentedCode, Vec<u8>) {
        let original_code_len = self.raw_code.len() as u32;
        (
            InstrumentedCode {
                code: self.code,
                original_code_len,
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
    original_code_len: u32,
    exports: BTreeSet<DispatchKind>,
    static_pages: WasmPage,
    version: u32,
}

impl InstrumentedCode {
    /// Returns reference to the instrumented binary code.
    pub fn code(&self) -> &[u8] {
        &self.code
    }

    /// Returns the length of the original binary code.
    pub fn original_code_len(&self) -> u32 {
        self.original_code_len
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
    pub fn static_pages(&self) -> WasmPage {
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

#[cfg(test)]
mod tests {
    use crate::code::{Code, CodeError};
    use alloc::vec::Vec;
    use gear_wasm_instrument::wasm_instrument::gas_metering::ConstantCostRules;

    fn wat2wasm(s: &str) -> Vec<u8> {
        wabt::Wat2Wasm::new()
            .validate(true)
            .convert(s)
            .unwrap()
            .as_ref()
            .to_vec()
    }

    #[test]
    fn reject_unknown_exports() {
        const WAT: &str = r#"
        (module
            (import "env" "memory" (memory 1))
            (export "this_import_is_unknown" (func $test))
            (func $test)
        )
        "#;

        let raw_code = wat2wasm(WAT);

        assert_eq!(
            Code::try_new(raw_code, 1, |_| ConstantCostRules::default(), None),
            Err(CodeError::NonGearExportFnFound)
        );
    }

    #[test]
    fn required_fn_not_found() {
        const WAT: &str = r#"
        (module
            (import "env" "memory" (memory 1))
            (export "handle_signal" (func $handle_signal))
            (func $handle_signal)
        )
        "#;

        let raw_code = wat2wasm(WAT);

        assert_eq!(
            Code::try_new(raw_code, 1, |_| ConstantCostRules::default(), None),
            Err(CodeError::RequiredExportFnNotFound)
        );
    }

    #[test]
    fn stack_limit_injection_works() {
        const WAT: &str = r#"
        (module
            (import "env" "memory" (memory 1))
            (export "init" (func $init))
            (func $init)
        )
        "#;

        let raw_code = wat2wasm(WAT);

        let _ = Code::try_new(
            raw_code,
            1,
            |_| ConstantCostRules::default(),
            Some(16 * 1024),
        )
        .unwrap();
    }
}
