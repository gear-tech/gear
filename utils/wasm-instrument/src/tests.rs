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

use super::*;
use crate::{
    rules::CustomConstantCostRules,
    syscalls::{ParamType::*, Ptr, RegularParamType::*, SyscallName},
};
use alloc::format;
use elements::Instruction::*;
use gas_metering::ConstantCostRules;
use parity_wasm::serialize;

fn inject<R, GetRulesFn>(
    module: elements::Module,
    get_gas_rules: GetRulesFn,
    module_name: &str,
) -> Result<Module, InstrumentationError>
where
    R: Rules,
    GetRulesFn: FnMut(&Module) -> R,
{
    InstrumentationBuilder::new(module_name)
        .with_gas_limiter(get_gas_rules)
        .instrument(module)
}

fn get_function_body(module: &elements::Module, index: usize) -> Option<&[elements::Instruction]> {
    module
        .code_section()
        .and_then(|code_section| code_section.bodies().get(index))
        .map(|func_body| func_body.code().elements())
}

fn prebuilt_simple_module() -> elements::Module {
    builder::module()
        .global()
        .value_type()
        .i32()
        .build()
        .function()
        .signature()
        .param()
        .i32()
        .build()
        .body()
        .build()
        .build()
        .function()
        .signature()
        .param()
        .i32()
        .build()
        .body()
        .with_instructions(elements::Instructions::new(vec![
            Call(0),
            If(elements::BlockType::NoResult),
            Call(0),
            Call(0),
            Call(0),
            Else,
            Call(0),
            Call(0),
            End,
            Call(0),
            End,
        ]))
        .build()
        .build()
        .build()
}

#[test]
fn simple_grow() {
    let module = parse_wat(
        r#"(module
        (func (result i32)
            global.get 0
            memory.grow)
        (global i32 (i32.const 42))
        (memory 0 1)
        )"#,
    );

    let injected_module = inject(module, |_| ConstantCostRules::new(1, 10_000, 0), "env").unwrap();

    // two new imports (index 0), the original func (i = 1), so
    // gas charge will occupy the next index.
    let gas_charge_index = 2;
    let grow_index = 3;

    assert_eq!(
        get_function_body(&injected_module, 0).unwrap(),
        &vec![
            I32Const(2),
            Call(gas_charge_index),
            GetGlobal(0),
            Call(grow_index),
            End
        ][..]
    );
    assert_eq!(
        get_function_body(&injected_module, 2).unwrap(),
        &vec![
            GetLocal(0),
            GetLocal(0),
            I32Const(10_000),
            I32Mul,
            Call(gas_charge_index),
            GrowMemory(0),
            End,
        ][..]
    );

    let binary = serialize(injected_module).expect("serialization failed");
    wasmparser::validate(&binary).unwrap();
}

#[test]
fn grow_no_gas_no_track() {
    let module = parse_wat(
        r"(module
        (func (result i32)
            global.get 0
            memory.grow)
        (global i32 (i32.const 42))
        (memory 0 1)
        )",
    );

    let injected_module = inject(module, |_| ConstantCostRules::default(), "env").unwrap();

    let gas_charge_index = 2;

    assert_eq!(
        get_function_body(&injected_module, 0).unwrap(),
        &vec![
            I32Const(2),
            Call(gas_charge_index),
            GetGlobal(0),
            GrowMemory(0),
            End
        ][..]
    );

    assert_eq!(injected_module.functions_space(), 3);

    let binary = serialize(injected_module).expect("serialization failed");
    wasmparser::validate(&binary).unwrap();
}

#[test]
fn duplicate_import() {
    let wat = format!(
        r#"(module
            (import "env" "{system_break}" (func))
            (func (result i32)
                global.get 0
                memory.grow)
            (global i32 (i32.const 42))
            (memory 0 1)
            )"#,
        system_break = SyscallName::SystemBreak.to_str()
    );
    let module = parse_wat(&wat);

    assert_eq!(
        inject(module, |_| ConstantCostRules::default(), "env"),
        Err(InstrumentationError::SystemBreakImportAlreadyExists)
    );
}

#[test]
fn duplicate_export() {
    let wat = format!(
        r#"(module
        (func (result i32)
            global.get 0
            memory.grow)
        (global (;0;) i32 (i32.const 42))
        (memory 0 1)
        (global (;1;) (mut i32) (i32.const 0))
        (export "{GLOBAL_NAME_GAS}" (global 0))
        )"#
    );
    let module = parse_wat(&wat);

    assert_eq!(
        inject(module, |_| ConstantCostRules::default(), "env"),
        Err(InstrumentationError::GasGlobalAlreadyExists)
    );
}

#[test]
fn unsupported_instruction() {
    let module = parse_wat(
        r#"(module
        (func (result f64)
            f64.const 10
            f64.const 3
            f64.div)
        (global i32 (i32.const 42))
        (memory 0 1)
        )"#,
    );

    assert_eq!(
        inject(module, |_| CustomConstantCostRules::default(), "env"),
        Err(InstrumentationError::GasInjection)
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
        &vec![
            I32Const(3),
            Call(gas_charge_index),
            Call(empty_func_index),
            If(elements::BlockType::NoResult),
            I32Const(3),
            Call(gas_charge_index),
            Call(empty_func_index),
            Call(empty_func_index),
            Call(empty_func_index),
            Else,
            I32Const(2),
            Call(gas_charge_index),
            Call(empty_func_index),
            Call(empty_func_index),
            End,
            Call(empty_func_index),
            End
        ][..]
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
        &vec![
            // (instruction_cost * 3) as i32 => ((2147483647 * 2) + 2147483647) as i32 =>
            // ((2147483647 + 2147483647 + 1) + 2147483646) as i32 =>
            // (u32::MAX as i32) + 2147483646 as i32
            I32Const(-1),
            Call(gas_charge_index),
            I32Const((instruction_cost - 1) as i32),
            Call(gas_charge_index),
            Call(empty_func_index),
            If(elements::BlockType::NoResult),
            // Same as upper
            I32Const(-1),
            Call(gas_charge_index),
            I32Const((instruction_cost - 1) as i32),
            Call(gas_charge_index),
            Call(empty_func_index),
            Call(empty_func_index),
            Call(empty_func_index),
            Else,
            // (instruction_cost * 2) as i32
            I32Const(-2),
            Call(gas_charge_index),
            Call(empty_func_index),
            Call(empty_func_index),
            End,
            Call(empty_func_index),
            End
        ][..]
    );
}

fn parse_wat(source: &str) -> elements::Module {
    let module_bytes = wat::parse_str(source).unwrap();
    elements::deserialize_buffer(module_bytes.as_ref()).unwrap()
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
    let syscalls = SyscallName::instrumentable();
    for syscall in syscalls {
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

// Basically checks that mutable error pointer is always last in every fallible syscall params set.
// WARNING: this test must never fail, unless a huge redesign in syscalls signatures has occurred.
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
