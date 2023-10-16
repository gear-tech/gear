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
    pages::WasmPagesAmount,
};
use alloc::{collections::BTreeSet, vec, vec::Vec};
use gear_wasm_instrument::{
    parity_wasm::{
        self,
        builder::ModuleBuilder,
        elements::{ExportEntry, GlobalEntry, GlobalType, InitExpr, Instruction, Internal, Module},
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

/// Defines maximal permitted amount of wasm memory pages.
///
/// Currently cannot be bigger than u32::MAX, cause of wasmer limitations.
pub const MAX_WASM_PAGES_AMOUNT: WasmPagesAmount = WasmPagesAmount::from_u16(65535);

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
    let Some(&stack_end_global_index) = get_export_global_index(module, STACK_END_EXPORT_NAME)
    else {
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
    /// Validation by wasmparser failed.
    #[display(fmt = "Wasm validation failed")]
    Validation,
    /// Error occurred during decoding original program code.
    #[display(fmt = "The wasm bytecode is failed to be decoded")]
    Decode,
    /// Error occurred during injecting gas metering instructions.
    ///
    /// This might be due to program contained unsupported/non-deterministic instructions
    /// (floats, memory grow, etc.).
    #[display(fmt = "Failed to inject instructions for gas metrics: may be in case \
        program contains unsupported instructions (floats, memory grow, etc.)")]
    GasInjection,
    /// Error occurred during stack height instrumentation.
    #[display(fmt = "Failed to set stack height limits")]
    StackLimitInjection,
    /// Error occurred during encoding instrumented program.
    #[display(fmt = "Failed to encode instrumented program")]
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Code {
    /// Code instrumented with the latest schedule.
    code: Vec<u8>,
    /// The uninstrumented, original version of the code.
    original_code: Vec<u8>,
    /// Exports of the wasm module.
    exports: BTreeSet<DispatchKind>,
    /// Static pages count from memory import.
    static_pages: WasmPagesAmount,
    /// Instruction weights version.
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

fn check_start_section(module: &Module) -> Result<(), CodeError> {
    if module.start_section().is_some() {
        log::debug!("Found start section in contract code, which is not allowed");
        Err(CodeError::StartSectionExists)
    } else {
        Ok(())
    }
}

fn export_stack_height(module: Module) -> Module {
    let globals = module
        .global_section()
        .expect("Global section must be create by `inject_stack_limiter` before")
        .entries()
        .len();
    ModuleBuilder::new()
        .with_module(module)
        .export()
        .field("__gear_stack_height")
        .internal()
        .global(globals as u32 - 1)
        .build()
        .build()
}

/// Configuration for `Code::try_new_mock_`.
/// By default all checks enabled.
pub struct TryNewCodeConfig {
    /// Instrumentation version
    pub version: u32,
    /// Stack height limit
    pub stack_height: Option<u32>,
    /// Export `__gear_stack_height` global
    pub export_stack_height: bool,
    /// Check exports (wasm contains init or handle exports)
    pub check_exports: bool,
    /// Check and canonize stack end
    pub check_and_canonize_stack_end: bool,
    /// Check mutable global exports
    pub check_mut_global_exports: bool,
    /// Check start section (not allowed for smart contracts)
    pub check_start_section: bool,
    /// Make wasmparser validation
    pub make_validation: bool,
}

impl Default for TryNewCodeConfig {
    fn default() -> Self {
        Self {
            version: 1,
            stack_height: None,
            export_stack_height: false,
            check_exports: true,
            check_and_canonize_stack_end: true,
            check_mut_global_exports: true,
            check_start_section: true,
            make_validation: true,
        }
    }
}

impl TryNewCodeConfig {
    /// New default config without exports checks.
    pub fn new_no_exports_check() -> Self {
        Self {
            check_exports: false,
            ..Default::default()
        }
    }
}

impl Code {
    /// Create the code by checking and instrumenting `original_code`.
    fn try_new_internal<R, GetRulesFn>(
        original_code: Vec<u8>,
        get_gas_rules: Option<GetRulesFn>,
        config: TryNewCodeConfig,
    ) -> Result<Self, CodeError>
    where
        R: Rules,
        GetRulesFn: FnMut(&Module) -> R,
    {
        if config.make_validation {
            wasmparser::validate(&original_code).map_err(|err| {
                log::trace!("Wasm validation failed: {err}");
                CodeError::Validation
            })?;
        }

        let mut module: Module =
            parity_wasm::deserialize_buffer(&original_code).map_err(|err| {
                log::trace!("The wasm bytecode is failed to be decoded: {err}");
                CodeError::Decode
            })?;

        if config.check_and_canonize_stack_end {
            check_and_canonize_gear_stack_end(&mut module)?;
        }
        if config.check_mut_global_exports {
            check_mut_global_exports(&module)?;
        }
        if config.check_start_section {
            check_start_section(&module)?;
        }

        // get initial memory size from memory import
        let static_pages = module
            .import_section()
            .ok_or(CodeError::ImportSectionNotFound)?
            .entries()
            .iter()
            .find_map(|entry| match entry.external() {
                parity_wasm::elements::External::Memory(mem_ty) => Some(mem_ty.limits().initial()),
                _ => None,
            })
            .map(WasmPagesAmount::try_from)
            .ok_or(CodeError::MemoryEntryNotFound)?
            .map_err(|_| CodeError::InvalidStaticPageCount)?;

        if static_pages > MAX_WASM_PAGES_AMOUNT {
            return Err(CodeError::InvalidStaticPageCount);
        }

        let exports = get_exports(&module, config.check_exports)?;
        if config.check_exports
            && !(exports.contains(&DispatchKind::Init) || exports.contains(&DispatchKind::Handle))
        {
            return Err(CodeError::RequiredExportFnNotFound);
        }

        if let Some(stack_limit) = config.stack_height {
            let globals = config.export_stack_height.then(|| module.globals_space());

            module = wasm_instrument::inject_stack_limiter(module, stack_limit).map_err(|err| {
                log::trace!("Failed to inject stack height limits: {err}");
                CodeError::StackLimitInjection
            })?;

            if let Some(globals_before) = globals {
                // ensure stack limiter injector has created global
                let globals_after = module.globals_space();
                assert_eq!(globals_after, globals_before + 1);

                module = export_stack_height(module);
            }
        }

        if let Some(mut get_gas_rules) = get_gas_rules {
            let gas_rules = get_gas_rules(&module);
            module = gear_wasm_instrument::inject(module, &gas_rules, "env")
                .map_err(|_| CodeError::GasInjection)?;
        }

        let code = parity_wasm::elements::serialize(module).map_err(|err| {
            log::trace!("Failed to encode instrumented program: {err}");
            CodeError::Encode
        })?;

        Ok(Self {
            code,
            original_code,
            exports,
            static_pages,
            instruction_weights_version: config.version,
        })
    }

    /// Create the code by checking and instrumenting `original_code`.
    /// Main logic of instrumentation can be represented by this example:
    /// Let's take a code:
    /// ```wasm
    /// (module
    ///    (import "env" "memory" (memory 1))
    ///    (export "init" (func $init))
    ///    (func $f1
    ///       <-- f1 code -->
    ///    )
    ///    (func $f2
    ///       if (i32.eqz (i32.const 0))
    ///          <-- some code -->
    ///       else
    ///          <-- some code -->
    ///       end
    ///    )
    ///    (func $f3
    ///       <-- f3 code -->
    ///    )
    ///    (func $init
    ///       call $f1
    ///       call $f2
    ///       call $f3
    ///       <-- some code -->
    ///    )
    /// )
    /// ```
    ///
    /// After instrumentation code will be like:
    /// ```wasm
    /// (module
    ///   (import "env" "memory" (memory 1))
    ///   (export "init" (func $init_export))
    ///   (func $gas_charge
    ///      <-- gas charge impl --> ;; see utils/wasm-instrument/src/lib.rs
    ///   )
    ///   (func $f1
    ///      i32.const 123
    ///      call $gas_charge
    ///      <-- f1 code -->
    ///   )
    ///   (func $f2
    ///      i32.const 123
    ///      call $gas_charge
    ///      if (i32.eqz (i32.const 0))
    ///         i32.const 1
    ///         call $gas_charge
    ///         <-- some code -->
    ///      else
    ///         i32.const 2
    ///         call $gas_charge
    ///         <-- some code -->
    ///      end
    ///   )
    ///   (func $init
    ///      i32.const 123
    ///      call $gas_charge
    ///      ;; stack limit check impl see in wasm_instrument::inject_stack_limiter
    ///      <-- stack limit check and increase -->
    ///      call $f1
    ///      <-- stack limit decrease -->
    ///      <-- stack limit check and increase -->
    ///      call $f2
    ///      <-- stack limit decrease -->
    ///      <-- some code -->
    ///   )
    ///   (func $init_export
    ///      i32.const 123
    ///      call $gas_charge
    ///      <-- stack limit check and increase -->
    ///      call $init
    ///      <-- stack limit decrease -->
    ///   )
    /// )
    pub fn try_new<R, GetRulesFn>(
        original_code: Vec<u8>,
        version: u32,
        get_gas_rules: GetRulesFn,
        stack_height: Option<u32>,
    ) -> Result<Self, CodeError>
    where
        R: Rules,
        GetRulesFn: FnMut(&Module) -> R,
    {
        Self::try_new_internal(
            original_code,
            Some(get_gas_rules),
            TryNewCodeConfig {
                version,
                stack_height,
                ..Default::default()
            },
        )
    }

    /// Create new code for mock goals with const or no instrumentation rules.
    pub fn try_new_mock_const_or_no_rules(
        original_code: Vec<u8>,
        const_rules: bool,
        config: TryNewCodeConfig,
    ) -> Result<Self, CodeError> {
        let get_gas_rules = const_rules.then_some(|_module: &Module| ConstantCostRules::default());
        Self::try_new_internal(original_code, get_gas_rules, config)
    }

    /// Create new code for mock goals with custom instrumentation rules.
    pub fn try_new_mock_with_rules<R, GetRulesFn>(
        original_code: Vec<u8>,
        get_gas_rules: GetRulesFn,
        config: TryNewCodeConfig,
    ) -> Result<Self, CodeError>
    where
        R: Rules,
        GetRulesFn: FnMut(&Module) -> R,
    {
        Self::try_new_internal(original_code, Some(get_gas_rules), config)
    }

    /// Returns the original code.
    pub fn original_code(&self) -> &[u8] {
        &self.original_code
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
    pub fn static_pages(&self) -> WasmPagesAmount {
        self.static_pages
    }

    /// Consumes this instance and returns the instrumented and raw binary codes.
    pub fn into_parts(self) -> (InstrumentedCode, Vec<u8>) {
        let original_code_len = self.original_code.len() as u32;
        (
            InstrumentedCode {
                code: self.code,
                original_code_len,
                exports: self.exports,
                static_pages: self.static_pages,
                version: self.instruction_weights_version,
            },
            self.original_code,
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
        let code_id = CodeId::generate(code.original_code());
        Self { code, code_id }
    }

    /// Creates the instance from the precalculated hash without checks.
    pub fn from_parts_unchecked(code: Code, code_id: CodeId) -> Self {
        debug_assert_eq!(code_id, CodeId::generate(code.original_code()));
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
    static_pages: WasmPagesAmount,
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
    pub fn static_pages(&self) -> WasmPagesAmount {
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

        let original_code = wat2wasm(WAT);

        assert_eq!(
            Code::try_new(original_code, 1, |_| ConstantCostRules::default(), None),
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

        let original_code = wat2wasm(WAT);

        assert_eq!(
            Code::try_new(original_code, 1, |_| ConstantCostRules::default(), None),
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

        let original_code = wat2wasm(WAT);

        let _ = Code::try_new(
            original_code,
            1,
            |_| ConstantCostRules::default(),
            Some(16 * 1024),
        )
        .unwrap();
    }
}
