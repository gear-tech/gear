// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
    message::DispatchKind,
    pages::{WasmPage, WasmPagesAmount},
};
use alloc::{collections::BTreeSet, vec::Vec};
use gear_wasm_instrument::{
    gas_metering::{CustomConstantCostRules, Rules},
    parity_wasm::{self, elements::Module},
    InstrumentationBuilder,
};

mod errors;
mod instrumented;
mod utils;

pub use errors::*;
pub use instrumented::*;
pub use utils::{ALLOWED_EXPORTS, MAX_WASM_PAGES_AMOUNT, REQUIRED_EXPORTS};

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
    /// Stack end page.
    stack_end: Option<WasmPage>,
    /// Instruction weights version.
    instruction_weights_version: u32,
}

/// Configuration for `Code::try_new_mock_`.
/// By default all checks enabled.
pub struct TryNewCodeConfig {
    /// Instrumentation version
    pub version: u32,
    /// Stack height limit
    pub stack_height: Option<u32>,
    /// Limit of data section amount
    pub data_segments_amount_limit: Option<u32>,
    /// Export `STACK_HEIGHT_EXPORT_NAME` global
    pub export_stack_height: bool,
    /// Check exports (wasm contains init or handle exports)
    pub check_exports: bool,
    /// Check imports (check that all imports are valid syscalls with correct signature)
    pub check_imports: bool,
    /// Check and canonize stack end
    pub check_and_canonize_stack_end: bool,
    /// Check mutable global exports
    pub check_mut_global_exports: bool,
    /// Check start section (not allowed for programs)
    pub check_start_section: bool,
    /// Check data section
    pub check_data_section: bool,
    /// Make wasmparser validation
    pub make_validation: bool,
}

impl Default for TryNewCodeConfig {
    fn default() -> Self {
        Self {
            version: 1,
            stack_height: None,
            data_segments_amount_limit: None,
            export_stack_height: false,
            check_exports: true,
            check_imports: true,
            check_and_canonize_stack_end: true,
            check_mut_global_exports: true,
            check_start_section: true,
            check_data_section: true,
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
            wasmparser::validate(&original_code).map_err(CodeError::Validation)?;
        }

        let mut module =
            parity_wasm::deserialize_buffer(&original_code).map_err(CodecError::Decode)?;

        let static_pages = utils::get_static_pages(&module)?;

        // Canonize stack end before any changes in module
        let stack_end = match config.check_and_canonize_stack_end {
            true => utils::check_and_canonize_gear_stack_end(&mut module, static_pages)?,
            false => None,
        };

        // Not changing steps
        if config.check_data_section {
            utils::check_data_section(
                &module,
                static_pages,
                stack_end,
                config.data_segments_amount_limit,
            )?;
        }
        if config.check_mut_global_exports {
            utils::check_mut_global_exports(&module)?;
        }
        if config.check_start_section {
            utils::check_start_section(&module)?;
        }
        if config.check_exports {
            utils::check_exports(&module)?;
        }
        if config.check_imports {
            utils::check_imports(&module)?;
        }

        // Get exports set before instrumentations.
        let exports = utils::get_exports(&module);

        let mut instrumentation_builder = InstrumentationBuilder::new("env");
        if let Some(stack_limit) = config.stack_height {
            instrumentation_builder.with_stack_limiter(stack_limit, config.export_stack_height);
        }
        if let Some(get_gas_rules) = get_gas_rules {
            instrumentation_builder.with_gas_limiter(get_gas_rules);
        }
        module = instrumentation_builder.instrument(module)?;

        let code = parity_wasm::elements::serialize(module).map_err(CodecError::Encode)?;

        Ok(Self {
            code,
            original_code,
            exports,
            static_pages,
            stack_end,
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
        data_segments_amount_limit: Option<u32>,
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
                data_segments_amount_limit,
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
        let get_gas_rules =
            const_rules.then_some(|_module: &Module| CustomConstantCostRules::default());
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
                stack_end: self.stack_end,
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

    /// Creates the instance from the hash and incompatible with that hash code.
    ///
    /// # Safety
    /// USE FOR TEST PURPOSES ONLY.
    pub unsafe fn from_incompatible_parts(code: Code, code_id: CodeId) -> Self {
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

#[cfg(test)]
mod tests {
    use crate::code::{Code, CodeError, DataSectionError, ExportError, ImportError, StackEndError};
    use alloc::{format, vec::Vec};
    use gear_wasm_instrument::{gas_metering::CustomConstantCostRules, STACK_END_EXPORT_NAME};

    fn wat2wasm_with_validate(s: &str, validate: bool) -> Vec<u8> {
        wabt::Wat2Wasm::new()
            .validate(validate)
            .convert(s)
            .unwrap()
            .as_ref()
            .to_vec()
    }

    fn wat2wasm(s: &str) -> Vec<u8> {
        wat2wasm_with_validate(s, true)
    }

    macro_rules! assert_code_err {
        ($res:expr, $expected:pat) => {
            let err = $res.expect_err("Code::try_new must return an error");
            let expected_err = stringify!($expected);
            assert!(
                matches!(err, $expected),
                "Must receive {:?}, got {:?}",
                expected_err,
                err
            );
        };
    }

    fn try_new_code_from_wat_with_params(
        wat: &str,
        stack_height: Option<u32>,
        data_segments_amount_limit: Option<u32>,
    ) -> Result<Code, CodeError> {
        Code::try_new(
            wat2wasm(wat),
            1,
            |_| CustomConstantCostRules::default(),
            stack_height,
            data_segments_amount_limit,
        )
    }

    fn try_new_code_from_wat(wat: &str, stack_height: Option<u32>) -> Result<Code, CodeError> {
        try_new_code_from_wat_with_params(wat, stack_height, None)
    }

    #[test]
    fn reject_unknown_exports() {
        let wat = r#"
            (module
                (import "env" "memory" (memory 1))
                (export "this_import_is_unknown" (func $test))
                (func $test)
            )
        "#;

        assert_code_err!(
            try_new_code_from_wat(wat, None),
            CodeError::Export(ExportError::ExcessExport(0))
        );
    }

    #[test]
    fn required_fn_not_found() {
        let wat = r#"
            (module
                (import "env" "memory" (memory 1))
                (export "handle_signal" (func $handle_signal))
                (func $handle_signal)
            )
        "#;

        assert_code_err!(
            try_new_code_from_wat(wat, None),
            CodeError::Export(ExportError::RequiredExportNotFound)
        );
    }

    #[test]
    fn stack_limit_injection_works() {
        let wat = r#"
            (module
                (import "env" "memory" (memory 1))
                (export "init" (func $init))
                (func $init)
            )
        "#;

        let _ = try_new_code_from_wat(wat, Some(16 * 1024)).unwrap();
    }

    #[test]
    fn data_segment_out_of_static_memory() {
        // Data segment end address is out of static memory.
        let wat = r#"
            (module
                (import "env" "memory" (memory 1))
                (export "init" (func $init))
                (func $init)
                (data (;0;) (i32.const 0x10000) "gear")
            )
        "#;

        assert_code_err!(
            try_new_code_from_wat(wat, None),
            CodeError::DataSection(DataSectionError::EndAddressOutOfStaticMemory(
                0x10000, 0x10003, 0x10000
            ))
        );

        // Data segment last byte is next byte after static memory
        let wat = r#"
            (module
                (import "env" "memory" (memory 1))
                (export "init" (func $init))
                (func $init)
                (data (;0;) (i32.const 0xfffd) "gear")
            )
        "#;

        assert_code_err!(
            try_new_code_from_wat(wat, None),
            CodeError::DataSection(DataSectionError::EndAddressOutOfStaticMemory(
                0xfffd, 0x10000, 0x10000
            ))
        );
    }

    #[test]
    fn data_segment_out_of_u32() {
        // Data segment end address is out of possible 32 bits address space.
        let wat = r#"
            (module
                (import "env" "memory" (memory 1))
                (export "init" (func $init))
                (func $init)
                (data (;0;) (i32.const 0xffffffff) "gear")
            )
        "#;

        assert_code_err!(
            try_new_code_from_wat(wat, None),
            CodeError::DataSection(DataSectionError::EndAddressOverflow(0xffffffff))
        );
    }

    #[test]
    fn data_segment_stack_overlaps() {
        // Data segment overlaps gear stack.
        let wat = format!(
            r#"
            (module
                (import "env" "memory" (memory 3))
                (export "init" (func $init))
                (func $init)
                (data (;0;) (i32.const 0x10000) "gear")
                (export "{STACK_END_EXPORT_NAME}" (global 0))
                (global (mut i32) (i32.const 0x20000))
            )"#
        );

        assert_code_err!(
            try_new_code_from_wat(wat.as_str(), None),
            CodeError::DataSection(DataSectionError::GearStackOverlaps(0x10000, 0x20000))
        );
    }

    #[test]
    fn data_section() {
        let wat = format!(
            r#"
            (module
                (import "env" "memory" (memory 3))
                (export "init" (func $init))
                (func $init)
                (data (i32.const 0x20000) "gear")
                (data (i32.const 0x10000) "")     ;; empty data segment
                (data (i32.const 0x1ffff) "gear") ;; overlapping other segments, also ok
                (data (i32.const 0x2ffff) "g")    ;; one byte before the end of memory
                (export "{STACK_END_EXPORT_NAME}" (global 0))
                (global (mut i32) (i32.const 0x10000))
            )"#
        );

        try_new_code_from_wat(wat.as_str(), None).expect("Must be ok");
    }

    #[test]
    fn check_mutable_global_exports_restriction() {
        let wat = r#"
            (module
                (import "env" "memory" (memory 0))
                (func $init)
                (export "init" (func $init))
                (export "global" (global 0))
                (global (;0;) (mut i32) (i32.const 0))
            )"#;

        assert_code_err!(
            try_new_code_from_wat(wat, None),
            CodeError::Export(ExportError::MutableGlobalExport(0, 1))
        );
    }

    #[test]
    fn stack_end_initialization() {
        let wat = format!(
            r#"
            (module
                (import "env" "memory" (memory 1))
                (import "env" "unknown" (global i32))
                (func $init)
                (export "init" (func $init))
                (export "{STACK_END_EXPORT_NAME}" (global 1))
                (global (mut i32) (global.get 0))
            )"#
        );

        assert_code_err!(
            try_new_code_from_wat(wat.as_str(), None),
            CodeError::StackEnd(StackEndError::Initialization)
        );
    }

    #[test]
    fn stack_end_alignment() {
        let wat = format!(
            r#"
            (module
                (import "env" "memory" (memory 2))
                (func $init)
                (export "init" (func $init))
                (export "{STACK_END_EXPORT_NAME}" (global 0))
                (global (;0;) (mut i32) (i32.const 0x10001))
            )"#
        );

        assert_code_err!(
            try_new_code_from_wat(wat.as_str(), None),
            CodeError::StackEnd(StackEndError::NotAligned(0x10001))
        );
    }

    #[test]
    fn stack_end_out_of_static_memory() {
        let wat = format!(
            r#"
            (module
                (import "env" "memory" (memory 1))
                (func $init)
                (export "init" (func $init))
                (export "{STACK_END_EXPORT_NAME}" (global 0))
                (global (;0;) (mut i32) (i32.const 0x20000))
            )"#
        );

        assert_code_err!(
            try_new_code_from_wat(wat.as_str(), None),
            CodeError::StackEnd(StackEndError::OutOfStatic(0x20000, 0x10000))
        );
    }

    #[test]
    fn stack_end() {
        let wat = format!(
            r#"
            (module
                (import "env" "memory" (memory 1))
                (func $init)
                (export "init" (func $init))
                (export "{STACK_END_EXPORT_NAME}" (global 0))
                (global (;0;) (mut i32) (i32.const 0x10000))
            )"#
        );

        let code = try_new_code_from_wat(wat.as_str(), None).expect("Must be ok");
        assert_eq!(code.stack_end, Some(1.into()));
    }

    #[test]
    fn export_to_imported_function() {
        let wat = r#"
            (module
                (import "env" "memory" (memory 1))
                (import "env" "gr_leave" (func $gr_leave))
                (export "init" (func $gr_leave))
                (func)
            )"#;

        assert_code_err!(
            try_new_code_from_wat(wat, None),
            CodeError::Export(ExportError::ExportReferencesToImportFunction(0, 0))
        );
    }

    #[test]
    fn export_to_imported_global() {
        let wat = r#"
            (module
                (import "env" "memory" (memory 1))
                (import "env" "global" (global i32))
                (export "init" (func 0))
                (export "global" (global 0))
                (func)
            )"#;

        assert_code_err!(
            try_new_code_from_wat(wat, None),
            CodeError::Export(ExportError::ExportReferencesToImportGlobal(1, 0))
        );
    }

    #[test]
    fn multi_memory_import() {
        let wat = r#"
            (module
                (import "env" "memory" (memory 1))
                (import "env" "memory2" (memory 2))
                (export "init" (func $init))
                (func $init)
            )
        "#;

        let res = Code::try_new(
            wat2wasm_with_validate(wat, false),
            1,
            |_| CustomConstantCostRules::default(),
            None,
            None,
        );

        assert_code_err!(res, CodeError::Validation(_));
    }

    #[test]
    fn global_import() {
        let wat = r#"
            (module
                (import "env" "memory" (memory 1))
                (import "env" "unknown" (global $unknown i32))
                (export "init" (func $init))
                (func $init)
            )
        "#;

        assert_code_err!(
            try_new_code_from_wat(wat, None),
            CodeError::Import(ImportError::UnexpectedImportKind {
                kind: &"Global",
                index: 1
            })
        );
    }

    #[test]
    fn table_import() {
        let wat = r#"
            (module
                (import "env" "memory" (memory 1))
                (import "env" "unknown" (table $unknown 10 20 funcref))
                (export "init" (func $init))
                (func $init)
            )
        "#;

        assert_code_err!(
            try_new_code_from_wat(wat, None),
            CodeError::Import(ImportError::UnexpectedImportKind {
                kind: &"Table",
                index: 1
            })
        );
    }

    #[test]
    fn data_segments_amount_limit() {
        const DATA_SEGMENTS_AMOUNT_LIMIT: u32 = 1024;

        let segment = r#"(data (i32.const 0x0) "gear")"#;

        let wat = format!(
            r#"
            (module
                (import "env" "memory" (memory 1))
                (func $init)
                (export "init" (func $init))
                {}
            )
        "#,
            segment.repeat(1025)
        );

        assert_code_err!(
            try_new_code_from_wat_with_params(
                wat.as_str(),
                None,
                DATA_SEGMENTS_AMOUNT_LIMIT.into()
            ),
            CodeError::DataSection(DataSectionError::DataSegmentsAmountLimit {
                limit: DATA_SEGMENTS_AMOUNT_LIMIT,
                actual: 1025
            })
        );
    }
}
