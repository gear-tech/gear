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

//! Module for checked code.

use crate::{
    ids::CodeId,
    message::{DispatchKind, WasmEntryPoint},
    pages::{PageNumber, PageU32Size, WasmPage},
};
use alloc::{collections::BTreeSet, vec, vec::Vec};
use gear_wasm_instrument::{
    parity_wasm::{
        self,
        elements::{ExportEntry, GlobalEntry, GlobalType, InitExpr, Instruction, Internal, Module},
    },
    wasm_instrument::{
        self as wasm_instrument,
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

fn get_export_entry<'a>(module: &'a Module, name: &str) -> Option<&'a ExportEntry> {
    module
        .export_section()?
        .entries()
        .iter()
        .find(|export| export.field() == name)
}

fn get_export_entry_mut<'a>(module: &'a mut Module, name: &str) -> Option<&'a mut ExportEntry> {
    module
        .export_section_mut()?
        .entries_mut()
        .iter_mut()
        .find(|export| export.field() == name)
}

fn get_export_global_index<'a>(module: &'a Module, name: &str) -> Option<&'a u32> {
    match get_export_entry(module, name)?.internal() {
        Internal::Global(index) => Some(index),
        _ => None,
    }
}

fn get_export_global_index_mut<'a>(module: &'a mut Module, name: &str) -> Option<&'a mut u32> {
    match get_export_entry_mut(module, name)?.internal_mut() {
        Internal::Global(index) => Some(index),
        _ => None,
    }
}

fn get_init_expr_const_i32(init_expr: &InitExpr) -> Option<i32> {
    let init_code = init_expr.code();
    if init_code.len() != 2 {
        return None;
    }
    match (&init_code[0], &init_code[1]) {
        (Instruction::I32Const(const_i32), Instruction::End) => Some(*const_i32),
        _ => None,
    }
}

fn get_global_entry(module: &Module, global_index: u32) -> Option<&GlobalEntry> {
    module
        .global_section()?
        .entries()
        .get(global_index as usize)
}

fn get_global_init_const_i32(module: &Module, global_index: u32) -> Result<i32, CodeError> {
    let init_expr = get_global_entry(module, global_index)
        .ok_or(CodeError::IncorrectGlobalIndex)?
        .init_expr();
    get_init_expr_const_i32(init_expr).ok_or(CodeError::StackEndInitialization)
}

fn check_and_canonize_gear_stack_end(module: &mut Module) -> Result<(), CodeError> {
    let Some(&stack_end_global_index) = get_export_global_index(module, STACK_END_EXPORT_NAME) else {
        return Ok(());
    };
    let stack_end_offset = get_global_init_const_i32(module, stack_end_global_index)?;

    // Checks, that each data segment does not overlap with stack.
    if let Some(data_section) = module.data_section() {
        for data_segment in data_section.entries() {
            let offset = data_segment
                .offset()
                .as_ref()
                .and_then(get_init_expr_const_i32)
                .ok_or(CodeError::DataSegmentInitialization)?;

            if offset < stack_end_offset {
                return Err(CodeError::StackEndOverlaps);
            }
        }
    };

    // If [STACK_END_EXPORT_NAME] points to mutable global, then make new const global
    // with the same init expr and change the export internal to point to the new global.
    if get_global_entry(module, stack_end_global_index)
        .ok_or(CodeError::IncorrectGlobalIndex)?
        .global_type()
        .is_mutable()
    {
        // Panic is impossible, because we have checked above, that global section exists.
        let global_section = module
            .global_section_mut()
            .unwrap_or_else(|| unreachable!("Cannot find global section"));
        let new_global_index = u32::try_from(global_section.entries().len())
            .map_err(|_| CodeError::IncorrectGlobalIndex)?;
        global_section.entries_mut().push(GlobalEntry::new(
            GlobalType::new(parity_wasm::elements::ValueType::I32, false),
            InitExpr::new(vec![
                Instruction::I32Const(stack_end_offset),
                Instruction::End,
            ]),
        ));

        // Panic is impossible, because we have checked above,
        // that stack end export exists and it points to global.
        get_export_global_index_mut(module, STACK_END_EXPORT_NAME)
            .map(|global_index| *global_index = new_global_index)
            .unwrap_or_else(|| unreachable!("Cannot find stack end export"))
    }

    Ok(())
}

/// Instrumentation error.
#[derive(Debug, PartialEq, Eq, derive_more::Display)]
pub enum CodeError {
    /// The provided code doesn't contain required import section.
    #[display(fmt = "Import section not found")]
    ImportSectionNotFound,
    /// The provided code doesn't contain memory entry section.
    #[display(fmt = "Memory entry not found")]
    MemoryEntryNotFound,
    /// The provided code doesn't contain export section.
    #[display(fmt = "Export section not found")]
    ExportSectionNotFound,
    /// The provided code doesn't contain the required `init` or `handle` export function.
    #[display(fmt = "Required export function `init` or `handle` not found")]
    RequiredExportFnNotFound,
    /// The provided code contains unnecessary function exports.
    #[display(fmt = "Unnecessary function exports found")]
    NonGearExportFnFound,
    /// Error occurred during decoding original program code.
    ///
    /// The provided code was a malformed Wasm bytecode or contained unsupported features
    /// (atomics, simd instructions, etc.).
    #[display(fmt = "The wasm bytecode is malformed or contains unsupported features")]
    Decode,
    /// Error occurred during injecting gas metering instructions.
    ///
    /// This might be due to program contained unsupported/non-deterministic instructions
    /// (floats, manual memory grow, etc.).
    #[display(fmt = "Failed to inject instructions for gas metrics: \
        program contains unsupported instructions (floats, manual memory grow, etc.)")]
    GasInjection,
    /// Error occurred during stack height instrumentation.
    #[display(fmt = "Failed to set stack height limits")]
    StackLimitInjection,
    /// Error occurred during encoding instrumented program.
    ///
    /// The only possible reason for that might be OOM.
    #[display(fmt = "Failed to encode instrumented program (probably because OOM)")]
    Encode,
    /// We restrict start sections in smart contracts.
    #[display(fmt = "Start section is not allowed for smart contracts")]
    StartSectionExists,
    /// The provided code has invalid count of static pages.
    #[display(fmt = "The wasm bytecode has invalid count of static pages")]
    InvalidStaticPageCount,
    /// Unsupported initialization of gear stack end global variable.
    #[display(fmt = "Unsupported initialization of gear stack end global variable")]
    StackEndInitialization,
    /// Unsupported initialization of data segment.
    #[display(fmt = "Unsupported initialization of data segment")]
    DataSegmentInitialization,
    /// Pointer to the stack end overlaps data segment.
    #[display(fmt = "Pointer to the stack end overlaps data segment")]
    StackEndOverlaps,
    /// Incorrect global export index. Can occur when export refers to not existing global index.
    #[display(fmt = "Global index in export is incorrect")]
    IncorrectGlobalIndex,
    /// Gear protocol restriction for now.
    #[display(fmt = "Program cannot have mutable globals in export section")]
    MutGlobalExport,
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

fn check_mut_global_exports(module: &Module) -> Result<(), CodeError> {
    let global_exports_indexes = module
        .export_section()
        .iter()
        .flat_map(|export_section| export_section.entries().iter())
        .filter_map(|export| match export.internal() {
            Internal::Global(index) => Some(*index as usize),
            _ => None,
        })
        .collect::<Vec<_>>();

    if global_exports_indexes.is_empty() {
        return Ok(());
    }

    if let Some(globals_section) = module.global_section() {
        for index in global_exports_indexes {
            if globals_section
                .entries()
                .get(index)
                .ok_or(CodeError::IncorrectGlobalIndex)?
                .global_type()
                .is_mutable()
            {
                return Err(CodeError::MutGlobalExport);
            }
        }
    }

    Ok(())
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

        let mut module: Module =
            parity_wasm::deserialize_buffer(&raw_code).map_err(|_| CodeError::Decode)?;

        check_and_canonize_gear_stack_end(&mut module)?;
        check_mut_global_exports(&module)?;

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
