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

//! Module for checked code.

use crate::{
    gas_metering::{CustomConstantCostRules, Rules},
    ids::{CodeId, prelude::*},
};
use alloc::vec::Vec;
use gear_wasm_instrument::{GEAR_SUPPORTED_FEATURES, InstrumentationBuilder, Module};

mod errors;
mod instrumented;
mod metadata;
mod utils;

pub use errors::*;
pub use instrumented::*;
pub use metadata::*;
pub use utils::{ALLOWED_EXPORTS, MAX_WASM_PAGES_AMOUNT, REQUIRED_EXPORTS};

use utils::CodeTypeSectionSizes;

/// Generic OS page size. Approximated to 4KB as a most common value.
const GENERIC_OS_PAGE_SIZE: u32 = 4096;

/// Configuration for `Code::try_new_mock_`.
/// By default all checks enabled.
pub struct TryNewCodeConfig {
    /// Instrumentation version
    pub version: u32,
    /// Stack height limit
    pub stack_height: Option<u32>,
    /// Limit of data section amount
    pub data_segments_amount_limit: Option<u32>,
    /// Limit on the number of tables.
    pub table_amount_limit: Option<u32>,
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
    /// Check table section
    pub check_table_section: bool,
    /// Make wasmparser validation
    pub make_validation: bool,
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

impl Default for TryNewCodeConfig {
    fn default() -> Self {
        Self {
            version: 1,
            stack_height: None,
            data_segments_amount_limit: None,
            table_amount_limit: None,
            export_stack_height: false,
            check_exports: true,
            check_imports: true,
            check_and_canonize_stack_end: true,
            check_mut_global_exports: true,
            check_start_section: true,
            check_data_section: true,
            check_table_section: true,
            make_validation: true,
        }
    }
}

/// Contains original and instrumented binary code of a program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Code {
    original: Vec<u8>,
    instrumented: InstrumentedCode,
    metadata: CodeMetadata,
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
            wasmparser::Validator::new_with_features(GEAR_SUPPORTED_FEATURES)
                .validate_all(&original_code)
                .map_err(CodeError::Validation)?;
        }

        let mut module = Module::new(&original_code)?;

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

        // Use instrumented module to get section sizes.
        let data_section_size = utils::get_data_section_size(&module)?;
        let global_section_size = utils::get_instantiated_global_section_size(&module)?;
        let table_section_size = utils::get_instantiated_table_section_size(&module);
        let element_section_size = utils::get_instantiated_element_section_size(&module)?;

        let code = module.serialize()?;

        // Use instrumented code to get section sizes.
        let CodeTypeSectionSizes {
            code_section,
            type_section,
        } = utils::get_code_type_sections_sizes(&code)?;

        let instantiated_section_sizes = InstantiatedSectionSizes::new(
            code_section,
            data_section_size,
            global_section_size,
            table_section_size,
            element_section_size,
            type_section,
        );

        let instrumented_code = InstrumentedCode::new(code, instantiated_section_sizes);

        let metadata = CodeMetadata::new(
            original_code.len() as u32,
            exports,
            static_pages,
            stack_end,
            InstrumentationStatus::Instrumented {
                version: config.version,
                code_len: instrumented_code.bytes().len() as u32,
            },
        );

        Ok(Self {
            original: original_code,
            instrumented: instrumented_code,
            metadata,
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
    /// ```
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
        &self.original
    }

    /// Returns the instrumented code.
    pub fn instrumented_code(&self) -> &InstrumentedCode {
        &self.instrumented
    }

    /// Returns the code metadata.
    pub fn metadata(&self) -> &CodeMetadata {
        &self.metadata
    }

    /// Consumes this instance and returns the instrumented and raw binary codes.
    pub fn into_parts(self) -> (Vec<u8>, InstrumentedCode, CodeMetadata) {
        (self.original, self.instrumented, self.metadata)
    }

    /// Consumes this instance and returns the instrumented code and metadata struct.
    pub fn into_instrumented_code_and_metadata(self) -> InstrumentedCodeAndMetadata {
        InstrumentedCodeAndMetadata {
            instrumented_code: self.instrumented,
            metadata: self.metadata,
        }
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

/// The newtype contains the InstrumentedCode instance and the corresponding metadata.
#[derive(Clone, Debug)]
pub struct InstrumentedCodeAndMetadata {
    /// Instrumented code.
    pub instrumented_code: InstrumentedCode,
    /// Code metadata.
    pub metadata: CodeMetadata,
}

impl InstrumentedCodeAndMetadata {
    /// Decomposes this instance into parts.
    pub fn into_parts(self) -> (InstrumentedCode, CodeMetadata) {
        (self.instrumented_code, self.metadata)
    }
}

impl From<Code> for InstrumentedCodeAndMetadata {
    fn from(code: Code) -> Self {
        let (_, instrumented_code, metadata) = code.into_parts();
        Self {
            instrumented_code,
            metadata,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        code::{
            Code, CodeError, DataSectionError, ExportError, GENERIC_OS_PAGE_SIZE, ImportError,
            StackEndError, TryNewCodeConfig, utils::REF_TYPE_SIZE,
        },
        gas_metering::CustomConstantCostRules,
    };
    use alloc::{format, vec::Vec};
    use gear_wasm_instrument::{InstrumentationError, ModuleError, STACK_END_EXPORT_NAME};

    fn wat2wasm_with_validate(s: &str, validate: bool) -> Vec<u8> {
        let code = wat::parse_str(s).unwrap();
        if validate {
            wasmparser::validate(&code).unwrap();
        }
        code
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
        make_validation: bool,
    ) -> Result<Code, CodeError> {
        Code::try_new_mock_const_or_no_rules(
            wat2wasm(wat),
            true,
            TryNewCodeConfig {
                stack_height,
                data_segments_amount_limit,
                make_validation,
                ..Default::default()
            },
        )
    }

    fn try_new_code_from_wat(wat: &str, stack_height: Option<u32>) -> Result<Code, CodeError> {
        try_new_code_from_wat_with_params(wat, stack_height, None, true)
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
        assert_eq!(code.metadata().stack_end(), Some(1.into()));
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
                DATA_SEGMENTS_AMOUNT_LIMIT.into(),
                true,
            ),
            CodeError::DataSection(DataSectionError::DataSegmentsAmountLimit {
                limit: DATA_SEGMENTS_AMOUNT_LIMIT,
                actual: 1025
            })
        );
    }

    #[test]
    fn data_section_bytes() {
        // Smoke
        let wat = r#"
            (module
                (import "env" "memory" (memory 3))
                (func $init)
                (export "init" (func $init))
                (data (i32.const 0x20000) "gear")
            )
        "#;

        assert_eq!(
            try_new_code_from_wat(wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .data_section(),
            GENERIC_OS_PAGE_SIZE,
        );

        // 2 adjacent
        let wat = r#"
            (module
                (import "env" "memory" (memory 3))
                (func $init)
                (export "init" (func $init))
                (data (i32.const 0x0000) "gear")
                (data (i32.const 0x1000) "gear")
            )
        "#;

        assert_eq!(
            try_new_code_from_wat(wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .data_section(),
            GENERIC_OS_PAGE_SIZE * 2,
        );

        // 2 not adjacent
        let wat = r#"
            (module
                (import "env" "memory" (memory 3))
                (func $init)
                (export "init" (func $init))
                (data (i32.const  0x0000) "gear")
                (data (i32.const 0x10000) "gear")
            )
        "#;

        assert_eq!(
            try_new_code_from_wat(wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .data_section(),
            GENERIC_OS_PAGE_SIZE * 2,
        );

        // 2 zero sized
        let wat = r#"
            (module
                (import "env" "memory" (memory 3))
                (func $init)
                (export "init" (func $init))
                (data (i32.const 0x0) "")
                (data (i32.const 0x0) "")
            )
        "#;

        assert_eq!(
            try_new_code_from_wat(wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .data_section(),
            0,
        );

        // Overlap
        let wat = r#"
            (module
                (import "env" "memory" (memory 3))
                (func $init)
                (export "init" (func $init))
                (data (i32.const 0x20000) "gear")
                (data (i32.const 0x20001) "gear")
            )
        "#;

        assert_eq!(
            try_new_code_from_wat(wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .data_section(),
            GENERIC_OS_PAGE_SIZE,
        );

        // Big segment
        let wat = format!(
            r#"
            (module
                (import "env" "memory" (memory 3))
                (func $init)
                (export "init" (func $init))
                (data (i32.const 0x20000) "{}")
            )
        "#,
            "a".repeat((GENERIC_OS_PAGE_SIZE + 1) as usize)
        );

        assert_eq!(
            try_new_code_from_wat(&wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .data_section(),
            GENERIC_OS_PAGE_SIZE * 2,
        );

        // 2 big segments
        let wat = format!(
            r#"
            (module
                (import "env" "memory" (memory 3))
                (func $init)
                (export "init" (func $init))
                (data (i32.const 0x20000) "{0}")
                (data (i32.const 0x23000) "{1}")
            )
            "#,
            "a".repeat((GENERIC_OS_PAGE_SIZE * 3) as usize),
            "b".repeat((GENERIC_OS_PAGE_SIZE) as usize)
        );

        assert_eq!(
            try_new_code_from_wat(&wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .data_section(),
            GENERIC_OS_PAGE_SIZE * 4,
        );

        // 2 big segments overlap
        let wat = format!(
            r#"
            (module
                (import "env" "memory" (memory 3))
                (func $init)
                (export "init" (func $init))
                (data (i32.const 0x20000) "{0}")
                (data (i32.const 0x21000) "{1}")
            )
            "#,
            "a".repeat((GENERIC_OS_PAGE_SIZE * 2 + 1) as usize),
            "b".repeat((GENERIC_OS_PAGE_SIZE * 2 + 1) as usize)
        );

        assert_eq!(
            try_new_code_from_wat(&wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .data_section(),
            GENERIC_OS_PAGE_SIZE * 4,
        );
    }

    #[test]
    fn code_section_bytes() {
        const INSTRUMENTATION_CODE_SIZE: u32 = 74;

        let wat = r#"
            (module
                (import "env" "memory" (memory 3))
                (func $init)
                (export "init" (func $init))
                (func $sum (param i32 i32) (result i32)
                    local.get 0
                    local.get 1
                    i32.add
                )
            )
        "#;

        assert_eq!(
            try_new_code_from_wat(wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .code_section(),
            INSTRUMENTATION_CODE_SIZE + 11,
        );
    }

    #[test]
    fn global_section_bytes() {
        const INSTRUMENTATION_GLOBALS_SIZE: usize = size_of::<i32>() + size_of::<i64>();

        let wat = r#"
            (module
                (import "env" "memory" (memory 3))
                (func $init)
                (export "init" (func $init))
                (global (mut i32) (i32.const 0))
                (global (mut i32) (i32.const 0))
                (global (mut i64) (i64.const 0))
            )
        "#;

        assert_eq!(
            try_new_code_from_wat(wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .global_section(),
            (INSTRUMENTATION_GLOBALS_SIZE + size_of::<i32>() * 2 + size_of::<i64>()) as u32,
        );
    }

    #[test]
    fn element_section_bytes() {
        let wat = r#"
            (module
                (import "env" "memory" (memory 3))
                (func $init)
                (export "init" (func $init))
                (table 10 10 funcref)
                (elem (i32.const 1) 0 0 0 0)
            )
        "#;

        assert_eq!(
            try_new_code_from_wat(wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .table_section(),
            10 * REF_TYPE_SIZE,
        );

        assert_eq!(
            try_new_code_from_wat(wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .element_section(),
            REF_TYPE_SIZE * 4,
        );
    }

    #[test]
    fn type_section_bytes() {
        let wat = r#"
            (module
                (import "env" "memory" (memory 3))
                (type (;35;) (func (param i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32) (result i32)))
                (type (;36;) (func (param i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32) (result i32)))
                (func $init)
                (export "init" (func $init))
            )
        "#;

        assert_eq!(
            try_new_code_from_wat(wat, Some(1024))
                .unwrap()
                .instrumented_code()
                .instantiated_section_sizes()
                .type_section(),
            50,
        );
    }

    #[test]
    fn unsupported_instruction() {
        // floats
        let res = try_new_code_from_wat_with_params(
            r#"
            (module
                (import "env" "memory" (memory 0 1))
                (func (result f64)
                    f64.const 10
                    f64.const 3
                    f64.div)
                (global i32 (i32.const 42))
                (func $init)
                (export "init" (func $init))
            )
            "#,
            Some(1024),
            None,
            // check not only `wasmparser` validator denies forbidden instructions
            false,
        );

        assert!(matches!(
            res,
            Err(CodeError::Module(ModuleError::UnsupportedInstruction(_))),
        ));

        // memory grow
        let res = try_new_code_from_wat(
            r#"
            (module
                (import "env" "memory" (memory 0 1))
                (func (result i32)
                    global.get 0
                    memory.grow
                )
                (global i32 (i32.const 42))
                (func $init)
                (export "init" (func $init))
        )"#,
            Some(1024),
        );

        assert!(matches!(
            res,
            Err(CodeError::Instrumentation(
                InstrumentationError::GasInjection
            ))
        ));
    }
}
