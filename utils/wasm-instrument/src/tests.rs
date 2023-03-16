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

use super::*;
use crate::syscalls::SysCallName;
use elements::Instruction::*;
use gas_metering::ConstantCostRules;
use parity_wasm::serialize;

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

    let injected_module = inject(module, &ConstantCostRules::new(1, 10_000, 0), "env").unwrap();

    // two new imports (indexes 0 & 1), the original func (i = 2), so
    // gas charge will occupy the next index.
    let gas_charge_index = 3;
    let grow_index = 4;

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

    let injected_module = inject(module, &ConstantCostRules::default(), "env").unwrap();

    let gas_charge_index = 3;

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

    assert_eq!(injected_module.functions_space(), 4);

    let binary = serialize(injected_module).expect("serialization failed");
    wasmparser::validate(&binary).unwrap();
}

#[test]
fn duplicate_import() {
    let wat = format!(
        r#"(module
            (import "env" "{out_of_gas}" (func))
            (func (result i32)
                global.get 0
                memory.grow)
            (global i32 (i32.const 42))
            (memory 0 1)
            )"#,
        out_of_gas = SysCallName::OutOfGas.to_str()
    );
    let module = parse_wat(&wat);

    assert!(inject(module, &ConstantCostRules::default(), "env").is_err());
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
        (export "{GLOBAL_NAME_ALLOWANCE}" (global 0))
        )"#
    );
    let module = parse_wat(&wat);

    assert!(inject(module, &ConstantCostRules::default(), "env").is_err());
}

#[test]
fn call_index() {
    let injected_module = inject(
        prebuilt_simple_module(),
        &ConstantCostRules::default(),
        "env",
    )
    .unwrap();

    let empty_func_index = 2;
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
        &ConstantCostRules::new(instruction_cost, 0, 0),
        "env",
    )
    .unwrap();

    let empty_func_index = 2;
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

            let injected_module = inject(input_module, &ConstantCostRules::default(), "env")
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
            (get_global 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 3 (i32.const 1))
            (get_global 0)))
    "#
}

test_gas_counter_injection! {
    name = nested;
    input = r#"
    (module
        (func (result i32)
            (get_global 0)
            (block
                (get_global 0)
                (get_global 0)
                (get_global 0))
            (get_global 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 3 (i32.const 6))
            (get_global 0)
            (block
                (get_global 0)
                (get_global 0)
                (get_global 0))
            (get_global 0)))
    "#
}

test_gas_counter_injection! {
    name = ifelse;
    input = r#"
    (module
        (func (result i32)
            (get_global 0)
            (if
                (then
                    (get_global 0)
                    (get_global 0)
                    (get_global 0))
                (else
                    (get_global 0)
                    (get_global 0)))
            (get_global 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 3 (i32.const 3))
            (get_global 0)
            (if
                (then
                    (call 3 (i32.const 3))
                    (get_global 0)
                    (get_global 0)
                    (get_global 0))
                (else
                    (call 3 (i32.const 2))
                    (get_global 0)
                    (get_global 0)))
            (get_global 0)))
    "#
}

test_gas_counter_injection! {
    name = branch_innermost;
    input = r#"
    (module
        (func (result i32)
            (get_global 0)
            (block
                (get_global 0)
                (drop)
                (br 0)
                (get_global 0)
                (drop))
            (get_global 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 3 (i32.const 6))
            (get_global 0)
            (block
                (get_global 0)
                (drop)
                (br 0)
                (call 3 (i32.const 2))
                (get_global 0)
                (drop))
            (get_global 0)))
    "#
}

test_gas_counter_injection! {
    name = branch_outer_block;
    input = r#"
    (module
        (func (result i32)
            (get_global 0)
            (block
                (get_global 0)
                (if
                    (then
                        (get_global 0)
                        (get_global 0)
                        (drop)
                        (br_if 1)))
                (get_global 0)
                (drop))
            (get_global 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 3 (i32.const 5))
            (get_global 0)
            (block
                (get_global 0)
                (if
                    (then
                        (call 3 (i32.const 4))
                        (get_global 0)
                        (get_global 0)
                        (drop)
                        (br_if 1)))
                (call 3 (i32.const 2))
                (get_global 0)
                (drop))
            (get_global 0)))
    "#
}

test_gas_counter_injection! {
    name = branch_outer_loop;
    input = r#"
    (module
        (func (result i32)
            (get_global 0)
            (loop
                (get_global 0)
                (if
                    (then
                        (get_global 0)
                        (br_if 0))
                    (else
                        (get_global 0)
                        (get_global 0)
                        (drop)
                        (br_if 1)))
                (get_global 0)
                (drop))
            (get_global 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 3 (i32.const 3))
            (get_global 0)
            (loop
                (call 3 (i32.const 4))
                (get_global 0)
                (if
                    (then
                        (call 3 (i32.const 2))
                        (get_global 0)
                        (br_if 0))
                    (else
                        (call 3 (i32.const 4))
                        (get_global 0)
                        (get_global 0)
                        (drop)
                        (br_if 1)))
                (get_global 0)
                (drop))
            (get_global 0)))
    "#
}

test_gas_counter_injection! {
    name = return_from_func;
    input = r#"
    (module
        (func (result i32)
            (get_global 0)
            (if
                (then
                    (return)))
            (get_global 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 3 (i32.const 2))
            (get_global 0)
            (if
                (then
                    (call 3 (i32.const 1))
                    (return)))
            (call 3 (i32.const 1))
            (get_global 0)))
    "#
}

test_gas_counter_injection! {
    name = branch_from_if_not_else;
    input = r#"
    (module
        (func (result i32)
            (get_global 0)
            (block
                (get_global 0)
                (if
                    (then (br 1))
                    (else (br 0)))
                (get_global 0)
                (drop))
            (get_global 0)))
    "#;
    expected = r#"
    (module
        (func (result i32)
            (call 3 (i32.const 5))
            (get_global 0)
            (block
                (get_global 0)
                (if
                    (then
                        (call 3 (i32.const 1))
                        (br 1))
                    (else
                        (call 3 (i32.const 1))
                        (br 0)))
                (call 3 (i32.const 2))
                (get_global 0)
                (drop))
            (get_global 0)))
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
            (call 3 (i32.const 2))
            (loop
                (call 3 (i32.const 1))
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
            (call 4 (i32.const 3))
            (call 2)
            (loop
                (call 4 (i32.const 1))
                (br 0)
            )
            unreachable
        )
    )
    "#
}

/// Check that all sys calls are supported by backend.
#[test]
fn test_sys_calls_table() {
    use gas_metering::ConstantCostRules;
    use gear_backend_common::{mock::MockExt, ActorTerminationReason, BackendReport, Environment};
    use gear_backend_wasmi::WasmiEnvironment;
    use gear_core::message::DispatchKind;
    use parity_wasm::builder;

    // Make module with one empty function.
    let mut module = builder::module()
        .function()
        .signature()
        .build()
        .build()
        .build();

    // Insert syscalls imports.
    for name in SysCallName::instrumentable() {
        let sign = name.signature();
        let types = module.type_section_mut().unwrap().types_mut();
        let type_no = types.len() as u32;
        types.push(parity_wasm::elements::Type::Function(sign.func_type()));

        module = builder::from_module(module)
            .import()
            .module("env")
            .external()
            .func(type_no)
            .field(name.to_str())
            .build()
            .build();
    }

    let module = inject(module, &ConstantCostRules::default(), "env").unwrap();
    let code = module.into_bytes().unwrap();

    // Execute wasm and check success.
    let ext = MockExt::default();
    let env = WasmiEnvironment::new(ext, &code, DispatchKind::Init, Default::default(), 0.into())
        .unwrap();
    let report = env
        .execute(|_, _, _| -> Result<(), u32> { Ok(()) })
        .unwrap();

    let BackendReport {
        termination_reason, ..
    } = report;

    assert_eq!(termination_reason, ActorTerminationReason::Success.into());
}
