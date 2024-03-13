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

use crate::{ids::CodeId, message::DispatchKind, pages::WasmPage};
use alloc::{collections::BTreeSet, vec::Vec};
use gear_wasm_instrument::{
    parity_wasm::{self, elements::Module},
    rules::CustomConstantCostRules,
    wasm_instrument::gas_metering::Rules,
    InstrumentationBuilder,
};

mod errors;
mod instrumented;
mod utils;

pub use errors::*;
pub use instrumented::*;
pub use utils::{ALLOWED_EXPORTS, MAX_WASM_PAGE_AMOUNT, REQUIRED_EXPORTS};

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
    static_pages: WasmPage,
    /// +_+_+
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
            utils::check_data_section(&module, static_pages, stack_end)?;
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
    pub fn static_pages(&self) -> WasmPage {
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
    use crate::code::{Code, CodeError, DataSectionError, ExportError, StackEndError};
    use alloc::{format, vec::Vec};
    use gear_wasm_instrument::{rules::CustomConstantCostRules, STACK_END_EXPORT_NAME};

    fn wat2wasm(s: &str) -> Vec<u8> {
        wabt::Wat2Wasm::new()
            .validate(true)
            .convert(s)
            .unwrap()
            .as_ref()
            .to_vec()
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

        assert_code_err!(
            Code::try_new(
                original_code,
                1,
                |_| CustomConstantCostRules::default(),
                None
            ),
            CodeError::Export(ExportError::ExcessExport(0))
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

        assert_code_err!(
            Code::try_new(
                original_code,
                1,
                |_| CustomConstantCostRules::default(),
                None
            ),
            CodeError::Export(ExportError::RequiredExportNotFound)
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
            |_| CustomConstantCostRules::default(),
            Some(16 * 1024),
        )
        .unwrap();
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
            Code::try_new(
                wat2wasm(wat),
                1,
                |_| CustomConstantCostRules::default(),
                None
            ),
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
            Code::try_new(
                wat2wasm(wat),
                1,
                |_| CustomConstantCostRules::default(),
                None
            ),
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
            Code::try_new(
                wat2wasm(wat),
                1,
                |_| CustomConstantCostRules::default(),
                None
            ),
            CodeError::DataSection(DataSectionError::EndAddressOverflow(0xffffffff))
        );
    }

    #[test]
    fn data_segment_stack_overlaps() {
        // Data segment overlaps gear stack.
        let wat = r#"
            (module
                (import "env" "memory" (memory 3))
                (export "init" (func $init))
                (func $init)
                (data (;0;) (i32.const 0x10000) "gear")
                (export "__gear_stack_end" (global 0))
                (global (mut i32) (i32.const 0x20000))
            )
        "#;
        assert_code_err!(
            Code::try_new(
                wat2wasm(wat),
                1,
                |_| CustomConstantCostRules::default(),
                None
            ),
            CodeError::DataSection(DataSectionError::GearStackOverlaps(0x10000, 0x20000))
        );
    }

    #[test]
    fn data_section() {
        let wat = r#"
            (module
                (import "env" "memory" (memory 3))
                (export "init" (func $init))
                (func $init)
                (data (i32.const 0x20000) "gear")
                (data (i32.const 0x10000) "")     ;; empty data segment
                (data (i32.const 0x1ffff) "gear") ;; overlapping other segments, also ok
                (data (i32.const 0x2ffff) "g")    ;; one byte before the end of memory
                (export "__gear_stack_end" (global 0))
                (global (mut i32) (i32.const 0x10000))
            )
        "#;

        Code::try_new(
            wat2wasm(wat),
            1,
            |_| CustomConstantCostRules::default(),
            None,
        )
        .expect("Must be ok");
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
            Code::try_new(
                wat2wasm(wat),
                1,
                |_| CustomConstantCostRules::default(),
                None
            ),
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
            Code::try_new(
                wat2wasm(wat.as_str()),
                1,
                |_| CustomConstantCostRules::default(),
                None
            ),
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
            Code::try_new(
                wat2wasm(wat.as_str()),
                1,
                |_| CustomConstantCostRules::default(),
                None
            ),
            CodeError::StackEnd(StackEndError::NotAligned)
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
            Code::try_new(
                wat2wasm(wat.as_str()),
                1,
                |_| CustomConstantCostRules::default(),
                None
            ),
            CodeError::StackEnd(StackEndError::OutOfStatic)
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

        let code = Code::try_new(
            wat2wasm(wat.as_str()),
            1,
            |_| CustomConstantCostRules::default(),
            None,
        )
        .expect("Must be ok");

        assert_eq!(code.stack_end, Some(1.into()));
    }
}
