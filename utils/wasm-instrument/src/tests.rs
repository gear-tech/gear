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

use crate::{
    gas_metering::ConstantCostRules,
    module::{Function, ModuleBuilder},
    syscalls::{ParamType::*, Ptr, RegularParamType::*, SyscallName},
    InstrumentationBuilder, InstrumentationError, Module, GLOBAL_NAME_GAS,
};
use alloc::format;
use gwasm_instrument::gas_metering::Rules;
use wasmparser::{
    BinaryReader, BlockType, FuncType, Global, GlobalType, Operator, Operator::*, ValType,
};

fn inject<'a, R, GetRulesFn>(
    module: Module<'a>,
    get_gas_rules: GetRulesFn,
    module_name: &'a str,
) -> Result<Module<'a>, InstrumentationError>
where
    R: Rules + 'a,
    GetRulesFn: FnMut(&Module) -> R + 'a,
{
    InstrumentationBuilder::new(module_name)
        .with_gas_limiter(get_gas_rules)
        .instrument(module)
}

fn get_function_body<'a>(module: &'a Module, index: usize) -> Option<&'a [Operator<'a>]> {
    module
        .code_section()
        .and_then(|code_section| code_section.get(index))
        .map(|func_body| func_body.instructions.as_ref())
}

fn prebuilt_simple_module() -> Module<'static> {
    let mut builder = ModuleBuilder::default();

    builder.push_global(Global {
        ty: GlobalType {
            content_type: ValType::I32,
            mutable: false,
            shared: false,
        },
        init_expr: wasmparser::ConstExpr::new(BinaryReader::new(&[], 0)),
    });

    builder.push_type(FuncType::new([ValType::I32], []));
    builder.push_function(Function {
        locals: Vec::new(),
        instructions: vec![],
    });

    builder.push_type(FuncType::new([ValType::I32], []));
    builder.push_function(Function {
        locals: Vec::new(),
        instructions: vec![
            Call { function_index: 0 },
            If {
                blockty: BlockType::Empty,
            },
            Call { function_index: 0 },
            Call { function_index: 0 },
            Call { function_index: 0 },
            Else,
            Call { function_index: 0 },
            Call { function_index: 0 },
            End,
            Call { function_index: 0 },
            End,
        ],
    });

    builder.build()
}

#[test]
fn duplicate_import() {
    let wat = format!(
        r#"(module
            (import "env" "{system_break}" (func))
            (func (result i32)
                global.get 0
            )
            (global i32 (i32.const 42))
            (memory 0 1)
            )"#,
        system_break = SyscallName::SystemBreak.to_str()
    );
    let module = parse_wat(&wat);

    assert_eq!(
        inject(module, |_| ConstantCostRules::default(), "env").unwrap_err(),
        InstrumentationError::SystemBreakImportAlreadyExists,
    );
}

#[test]
fn duplicate_export() {
    let wat = format!(
        r#"(module
        (func (result i32)
            global.get 0
        )
        (global (;0;) i32 (i32.const 42))
        (memory 0 1)
        (global (;1;) (mut i32) (i32.const 0))
        (export "{GLOBAL_NAME_GAS}" (global 0))
        )"#
    );
    let module = parse_wat(&wat);

    assert_eq!(
        inject(module, |_| ConstantCostRules::default(), "env").unwrap_err(),
        InstrumentationError::GasGlobalAlreadyExists
    );
}

#[test]
fn call_index() {
    let injected_module = inject(
        prebuilt_simple_module(),
        |_| ConstantCostRules::default(),
        "env",
    )
    .unwrap();

    let empty_func_index = 1;
    let func_index = empty_func_index + 1;
    let gas_charge_index = func_index + 1;

    assert_eq!(
        get_function_body(&injected_module, 1).unwrap(),
        [
            I32Const { value: 3 },
            Call {
                function_index: gas_charge_index
            },
            Call {
                function_index: empty_func_index
            },
            If {
                blockty: BlockType::Empty
            },
            I32Const { value: 3 },
            Call {
                function_index: gas_charge_index
            },
            Call {
                function_index: empty_func_index
            },
            Call {
                function_index: empty_func_index
            },
            Call {
                function_index: empty_func_index
            },
            Else,
            I32Const { value: 2 },
            Call {
                function_index: gas_charge_index
            },
            Call {
                function_index: empty_func_index
            },
            Call {
                function_index: empty_func_index
            },
            End,
            Call {
                function_index: empty_func_index
            },
            End
        ]
    );
}

#[test]
fn cost_overflow() {
    let instruction_cost = u32::MAX / 2;
    let injected_module = inject(
        prebuilt_simple_module(),
        |_| ConstantCostRules::new(instruction_cost, 0, 0),
        "env",
    )
    .unwrap();

    let empty_func_index = 1;
    let func_index = empty_func_index + 1;
    let gas_charge_index = func_index + 1;

    assert_eq!(
        get_function_body(&injected_module, 1).unwrap(),
        &[
            // (instruction_cost * 3) as i32 => ((2147483647 * 2) + 2147483647) as i32 =>
            // ((2147483647 + 2147483647 + 1) + 2147483646) as i32 =>
            // (u32::MAX as i32) + 2147483646 as i32
            I32Const { value: -1 },
            Call {
                function_index: gas_charge_index
            },
            I32Const {
                value: (instruction_cost - 1) as i32
            },
            Call {
                function_index: gas_charge_index
            },
            Call {
                function_index: empty_func_index
            },
            If {
                blockty: BlockType::Empty
            },
            // Same as upper
            I32Const { value: -1 },
            Call {
                function_index: gas_charge_index
            },
            I32Const {
                value: (instruction_cost - 1) as i32
            },
            Call {
                function_index: gas_charge_index
            },
            Call {
                function_index: empty_func_index
            },
            Call {
                function_index: empty_func_index
            },
            Call {
                function_index: empty_func_index
            },
            Else,
            // (instruction_cost * 2) as i32
            I32Const { value: -2 },
            Call {
                function_index: gas_charge_index
            },
            Call {
                function_index: empty_func_index
            },
            Call {
                function_index: empty_func_index
            },
            End,
            Call {
                function_index: empty_func_index
            },
            End
        ]
    );
}

fn parse_wat<'a>(source: &str) -> Module<'a> {
    let module_bytes = wat::parse_str(source).unwrap();
    Module::new(module_bytes).unwrap()
}

macro_rules! test_gas_counter_injection {
    (name = $name:ident; input = $input:expr; expected = $expected:expr) => {
        #[test]
        fn $name() {
            let input_module = parse_wat($input);
            let expected_module = parse_wat($expected);

            let injected_module = inject(input_module, |_| ConstantCostRules::default(), "env")
                .expect("inject_gas_counter call failed");

            let actual_func_body = get_function_body(&injected_module, 0)
                .expect("injected module must have a function body");
            let expected_func_body = get_function_body(&expected_module, 0)
                .expect("post-module must have a function body");

            assert_eq!(actual_func_body, expected_func_body);
        }
    };
}

test_gas_counter_injection! {
    name = simple;
    input = r#"
    (module
        (func (result i32)
            (global.get 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 2 (i32.const 1))
            (global.get 0)))
    "#
}

test_gas_counter_injection! {
    name = nested;
    input = r#"
    (module
        (func (result i32)
            (global.get 0)
            (block
                (global.get 0)
                (global.get 0)
                (global.get 0))
            (global.get 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 2 (i32.const 6))
            (global.get 0)
            (block
                (global.get 0)
                (global.get 0)
                (global.get 0))
            (global.get 0)))
    "#
}

test_gas_counter_injection! {
    name = ifelse;
    input = r#"
    (module
        (func (result i32)
            (global.get 0)
            (if
                (then
                    (global.get 0)
                    (global.get 0)
                    (global.get 0))
                (else
                    (global.get 0)
                    (global.get 0)))
            (global.get 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 2 (i32.const 3))
            (global.get 0)
            (if
                (then
                    (call 2 (i32.const 3))
                    (global.get 0)
                    (global.get 0)
                    (global.get 0))
                (else
                    (call 2 (i32.const 2))
                    (global.get 0)
                    (global.get 0)))
            (global.get 0)))
    "#
}

test_gas_counter_injection! {
    name = branch_innermost;
    input = r#"
    (module
        (func (result i32)
            (global.get 0)
            (block
                (global.get 0)
                (drop)
                (br 0)
                (global.get 0)
                (drop))
            (global.get 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 2 (i32.const 6))
            (global.get 0)
            (block
                (global.get 0)
                (drop)
                (br 0)
                (call 2 (i32.const 2))
                (global.get 0)
                (drop))
            (global.get 0)))
    "#
}

test_gas_counter_injection! {
    name = branch_outer_block;
    input = r#"
    (module
        (func (result i32)
            (global.get 0)
            (block
                (global.get 0)
                (if
                    (then
                        (global.get 0)
                        (global.get 0)
                        (drop)
                        (br_if 1)))
                (global.get 0)
                (drop))
            (global.get 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 2 (i32.const 5))
            (global.get 0)
            (block
                (global.get 0)
                (if
                    (then
                        (call 2 (i32.const 4))
                        (global.get 0)
                        (global.get 0)
                        (drop)
                        (br_if 1)))
                (call 2 (i32.const 2))
                (global.get 0)
                (drop))
            (global.get 0)))
    "#
}

test_gas_counter_injection! {
    name = branch_outer_loop;
    input = r#"
    (module
        (func (result i32)
            (global.get 0)
            (loop
                (global.get 0)
                (if
                    (then
                        (global.get 0)
                        (br_if 0))
                    (else
                        (global.get 0)
                        (global.get 0)
                        (drop)
                        (br_if 1)))
                (global.get 0)
                (drop))
            (global.get 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 2 (i32.const 3))
            (global.get 0)
            (loop
                (call 2 (i32.const 4))
                (global.get 0)
                (if
                    (then
                        (call 2 (i32.const 2))
                        (global.get 0)
                        (br_if 0))
                    (else
                        (call 2 (i32.const 4))
                        (global.get 0)
                        (global.get 0)
                        (drop)
                        (br_if 1)))
                (global.get 0)
                (drop))
            (global.get 0)))
    "#
}

test_gas_counter_injection! {
    name = return_from_func;
    input = r#"
    (module
        (func (result i32)
            (global.get 0)
            (if
                (then
                    (return)))
            (global.get 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 2 (i32.const 2))
            (global.get 0)
            (if
                (then
                    (call 2 (i32.const 1))
                    (return)))
            (call 2 (i32.const 1))
            (global.get 0)))
    "#
}

test_gas_counter_injection! {
    name = branch_from_if_not_else;
    input = r#"
    (module
        (func (result i32)
            (global.get 0)
            (block
                (global.get 0)
                (if
                    (then (br 1))
                    (else (br 0)))
                (global.get 0)
                (drop))
            (global.get 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 2 (i32.const 5))
            (global.get 0)
            (block
                (global.get 0)
                (if
                    (then
                        (call 2 (i32.const 1))
                        (br 1))
                    (else
                        (call 2 (i32.const 1))
                        (br 0)))
                (call 2 (i32.const 2))
                (global.get 0)
                (drop))
            (global.get 0)))
    "#
}

test_gas_counter_injection! {
    name = empty_loop;
    input = r#"
    (module
        (func
            (loop
                (br 0)
            )
            unreachable
        )
    )
    "#;
    expected = r#"
    (module
        (func
            (call 2 (i32.const 2))
            (loop
                (call 2 (i32.const 1))
                (br 0)
            )
            unreachable
        )
    )
    "#
}

test_gas_counter_injection! {
    name = two_functions;
    input = r#"
    (module
        (func)
        (func
            (call 0)
            (loop
                (br 0)
            )
            unreachable
        )
    )
    "#;
    expected = r#"
    (module
        (func)
        (func
            (call 3 (i32.const 3))
            (call 1)
            (loop
                (call 3 (i32.const 1))
                (br 0)
            )
            unreachable
        )
    )
    "#
}

#[test]
fn check_memory_array_pointers_definition_correctness() {
    for syscall in SyscallName::instrumentable() {
        let signature = syscall.signature();
        let size_param_indexes = signature
            .params()
            .iter()
            .filter_map(|param_ty| match param_ty {
                Regular(Pointer(Ptr::SizedBufferStart { length_param_idx })) => {
                    Some(*length_param_idx)
                }
                _ => None,
            });

        for idx in size_param_indexes {
            assert_eq!(signature.params().get(idx), Some(&Regular(Length)));
        }
    }
}

/// Basically checks that mutable error pointer is always last in every fallible
/// syscall params set.
///
/// WARNING: this test must never fail, unless a huge redesign in syscalls
/// signatures has occurred.
#[test]
fn check_syscall_err_ptr_position() {
    for syscall in SyscallName::instrumentable() {
        if syscall.is_fallible() {
            let signature = syscall.signature();
            let err_ptr = signature
                .params()
                .last()
                .expect("fallible syscall has at least err ptr");
            assert!(matches!(err_ptr, Error(_)));
        }
    }
}
