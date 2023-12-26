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

use crate::wasm::PageCount as WasmPageCount;
use gear_wasm_instrument::{
    parity_wasm::{
        builder,
        elements::{
            BlockType, External, FuncBody, ImportCountType, Instruction, Instructions, Internal,
            Module, Section, Type, ValueType,
        },
    },
    syscalls::SyscallName,
    wasm_instrument::{self, InjectionConfig},
};
use gsys::HashWithValue;
use std::{
    collections::{BTreeMap, BTreeSet},
    mem, slice,
};

const PREALLOCATE: usize = 1_000;

enum Color {
    Grey,
    Black,
}

/// Remove call recursions in `module` by using mock functions.
pub fn remove_recursion(module: Module) -> Module {
    if module.code_section().is_none() {
        return module;
    }

    let mut calls_to_change = BTreeMap::<_, BTreeSet<_>>::new();
    let mut call_substitutions = BTreeMap::<_, _>::new();
    find_recursion(&module, |path, call| {
        let call_to_change = path.last().unwrap();

        call_substitutions.insert(call, u32::MAX);
        match calls_to_change.get_mut(call_to_change) {
            Some(calls) => {
                calls.insert(call as u32);
            }
            None => {
                let mut calls = BTreeSet::new();
                calls.insert(call as u32);

                calls_to_change.insert(*call_to_change, calls);
            }
        }
    });

    let import_count = module.import_count(ImportCountType::Function);

    let signature_entries = module.function_section().unwrap().entries().to_vec();
    let types = module.type_section().unwrap().types().to_vec();

    let mut mbuilder = builder::from_module(module);

    // generate mock functions with empty bodies
    let keys = call_substitutions.keys().cloned().collect::<Vec<_>>();
    for call_index in keys {
        let index = call_index - import_count;

        let signature_index = &signature_entries[index];
        let signature = &types[signature_index.type_ref() as usize];
        let Type::Function(signature) = signature;
        let results = signature.results();
        let mut body = Vec::with_capacity(results.len() + 1);
        for result in results {
            let instruction = match result {
                ValueType::I32 => Instruction::I32Const(u32::MAX as i32),
                ValueType::I64 => Instruction::I64Const(u64::MAX as i64),
                ValueType::F32 | ValueType::F64 => unreachable!("f32/64 types are not supported"),
            };

            body.push(instruction);
        }

        body.push(Instruction::End);

        let mock_index = mbuilder
            .push_function(
                builder::function()
                    .signature()
                    .with_params(signature.params().to_vec())
                    .with_results(signature.results().to_vec())
                    .build()
                    .body()
                    .with_instructions(Instructions::new(body))
                    .build()
                    .build(),
            )
            .body;

        call_substitutions.insert(call_index, mock_index + import_count as u32);
    }

    // change call indices to mock functions to disable recursion
    let mut module = mbuilder.build();
    let code_section = module.code_section_mut().unwrap();
    let function_bodies = code_section.bodies_mut();
    for (call_to_change, calls) in calls_to_change {
        let index = call_to_change - import_count;
        let function_body = &mut function_bodies[index];
        for instruction in function_body.code_mut().elements_mut().iter_mut() {
            let call_index = match instruction {
                Instruction::Call(i) if calls.contains(i) => i,
                _ => continue,
            };

            let i = *call_index as usize;
            let mock_index = *call_substitutions.get(&i).unwrap();
            *call_index = mock_index;
        }
    }

    module
}

/// Find possible call recursions in `module`. Calls `callback` with
/// functions indexes chain and an index creating a recursion.
///
/// Used algorithm is based on Depth-First Search (DFS) algorithm for
/// loops detection in graphs.
pub fn find_recursion<Callback>(module: &Module, mut callback: Callback)
where
    Callback: FnMut(&[usize], usize),
{
    let function_bodies = match module.code_section() {
        Some(s) if !s.bodies().is_empty() => s.bodies(),
        _ => return,
    };

    let import_count = module.import_count(ImportCountType::Function);

    let mut colored_list = Vec::<BTreeMap<_, _>>::with_capacity(function_bodies.len());
    let mut path = Vec::with_capacity(PREALLOCATE);

    for i in 0..function_bodies.len() {
        let call_index = import_count + i;
        let call_colored = colored_list
            .iter()
            .any(|colored| colored.contains_key(&call_index));

        if call_colored {
            continue;
        }

        let mut colored = Default::default();
        find_recursion_impl(
            call_index,
            import_count,
            function_bodies,
            &mut colored,
            &mut path,
            &mut callback,
        );
        colored_list.push(colored);
    }
}

fn find_recursion_impl<Callback>(
    call_index: usize,
    import_count: usize,
    function_bodies: &[FuncBody],
    colored: &mut BTreeMap<usize, Color>,
    path: &mut Vec<usize>,
    callback: &mut Callback,
) where
    Callback: FnMut(&[usize], usize),
{
    path.push(call_index);
    colored.insert(call_index, Color::Grey);

    let body_index = call_index - import_count;
    let body = &function_bodies[body_index];
    let instructions = body.code();
    for instruction in instructions.elements() {
        let called_index = match instruction {
            Instruction::Call(i) => *i as usize,
            _ => continue,
        };

        // imported function maybe called there
        if called_index < import_count {
            continue;
        }

        if colored.get(&called_index).is_none() {
            find_recursion_impl(
                called_index,
                import_count,
                function_bodies,
                colored,
                path,
                callback,
            );
        }

        if matches!(colored.get(&called_index), Some(Color::Grey)) {
            callback(path, called_index);
        }
    }

    colored.insert(call_index, Color::Black);
    path.pop();
}

pub fn inject_stack_limiter(module: Module) -> Module {
    wasm_instrument::inject_stack_limiter_with_config(
        module,
        InjectionConfig {
            stack_limit: 30_003,
            injection_fn: |signature| {
                let results = signature.results();
                let mut body = Vec::with_capacity(results.len() + 1);

                for result in results {
                    let instruction = match result {
                        ValueType::I32 => Instruction::I32Const(u32::MAX as i32),
                        ValueType::I64 => Instruction::I64Const(u64::MAX as i64),
                        ValueType::F32 | ValueType::F64 => {
                            unreachable!("f32/64 types are not supported")
                        }
                    };

                    body.push(instruction);
                }

                body.push(Instruction::Return);

                body
            },
            stack_height_export_name: None,
        },
    )
    .expect("Failed to inject stack height limits")
}

/// Injects a critical gas limit to a given wasm module.
///
/// Code before injection gas limiter:
/// ```ignore
/// fn func() {
///     func();
///     loop { }
/// }
/// ```
///
/// Code after injection gas limiter:
/// ```ignore
/// use gcore::exec;
///
/// const CRITICAL_GAS_LIMIT: u64 = 1_000_000;
///
/// fn func() {
///     // exit from recursions
///     if exec::gas_available() <= CRITICAL_GAS_LIMIT {
///         return;
///     }
///     func();
///     loop {
///         // exit from heavy loops
///         if exec::gas_available() <= CRITICAL_GAS_LIMIT {
///             return;
///         }
///     }
/// }
/// ```
pub fn inject_critical_gas_limit(module: Module, critical_gas_limit: u64) -> Module {
    // get initial memory size of program
    let Some(mem_size) = module
        .import_section()
        .and_then(|section| {
            section
                .entries()
                .iter()
                .find_map(|entry| match entry.external() {
                    External::Memory(mem_ty) => Some(mem_ty.limits().initial()),
                    _ => None,
                })
        })
        .map(Into::<WasmPageCount>::into)
        .map(|page_count| page_count.memory_size())
    else {
        return module;
    };

    // store available gas pointer on the last memory page
    let gas_ptr = mem_size - mem::size_of::<u64>() as u32;

    // add gr_gas_available import if needed
    let maybe_gr_gas_available_index = module.import_section().and_then(|section| {
        section
            .entries()
            .iter()
            .filter(|entry| matches!(entry.external(), External::Function(_)))
            .enumerate()
            .find_map(|(i, entry)| {
                (entry.module() == "env" && entry.field() == SyscallName::GasAvailable.to_str())
                    .then_some(i as u32)
            })
    });
    // sections should only be rewritten if the module did not previously have gr_gas_available import
    let rewrite_sections = maybe_gr_gas_available_index.is_none();

    let (gr_gas_available_index, mut module) = match maybe_gr_gas_available_index {
        Some(gr_gas_available_index) => (gr_gas_available_index, module),
        None => {
            let mut mbuilder = builder::from_module(module);

            // fn gr_gas_available(gas: *mut u64);
            let import_sig = mbuilder
                .push_signature(builder::signature().with_param(ValueType::I32).build_sig());

            mbuilder.push_import(
                builder::import()
                    .module("env")
                    .field(SyscallName::GasAvailable.to_str())
                    .external()
                    .func(import_sig)
                    .build(),
            );

            // back to plain module
            let module = mbuilder.build();

            let import_count = module.import_count(ImportCountType::Function);
            let gr_gas_available_index = import_count as u32 - 1;

            (gr_gas_available_index, module)
        }
    };

    let (Some(type_section), Some(function_section)) =
        (module.type_section(), module.function_section())
    else {
        return module;
    };

    let types = type_section.types().to_vec();
    let signature_entries = function_section.entries().to_vec();

    let Some(code_section) = module.code_section_mut() else {
        return module;
    };

    for (index, func_body) in code_section.bodies_mut().iter_mut().enumerate() {
        let signature_index = &signature_entries[index];
        let signature = &types[signature_index.type_ref() as usize];
        let Type::Function(signature) = signature;
        let results = signature.results();

        // create the body of the gas limiter:
        let mut body = Vec::with_capacity(results.len() + 9);
        body.extend_from_slice(&[
            // gr_gas_available(gas_ptr)
            Instruction::I32Const(gas_ptr as i32),
            Instruction::Call(gr_gas_available_index),
            // gas_available = *gas_ptr
            Instruction::I32Const(gas_ptr as i32),
            Instruction::I64Load(3, 0),
            Instruction::I64Const(critical_gas_limit as i64),
            // if gas_available <= critical_gas_limit { return result; }
            Instruction::I64LeU,
            Instruction::If(BlockType::NoResult),
        ]);

        // exit the current function with dummy results
        for result in results {
            let instruction = match result {
                ValueType::I32 => Instruction::I32Const(u32::MAX as i32),
                ValueType::I64 => Instruction::I64Const(u64::MAX as i64),
                ValueType::F32 | ValueType::F64 => unreachable!("f32/64 types are not supported"),
            };

            body.push(instruction);
        }

        body.extend_from_slice(&[Instruction::Return, Instruction::End]);

        let instructions = func_body.code_mut().elements_mut();

        let original_instructions =
            mem::replace(instructions, Vec::with_capacity(instructions.len()));
        let new_instructions = instructions;

        // insert gas limiter at the beginning of each function to limit recursions
        new_instructions.extend_from_slice(&body);

        // also insert gas limiter at the beginning of each block, loop and condition
        // to limit control instructions
        for instruction in original_instructions {
            match instruction {
                Instruction::Block(_) | Instruction::Loop(_) | Instruction::If(_) => {
                    new_instructions.push(instruction);
                    new_instructions.extend_from_slice(&body);
                }
                Instruction::Call(call_index)
                    if rewrite_sections && call_index >= gr_gas_available_index =>
                {
                    // fix function indexes if import gr_gas_available was inserted
                    new_instructions.push(Instruction::Call(call_index + 1));
                }
                _ => {
                    new_instructions.push(instruction);
                }
            }
        }
    }

    // fix other sections if import gr_gas_available was inserted
    if rewrite_sections {
        let sections = module.sections_mut();
        sections.retain(|section| !matches!(section, Section::Custom(_)));

        for section in sections {
            match section {
                Section::Export(export_section) => {
                    for export in export_section.entries_mut() {
                        if let Internal::Function(func_index) = export.internal_mut() {
                            if *func_index >= gr_gas_available_index {
                                *func_index += 1
                            }
                        }
                    }
                }
                Section::Element(elements_section) => {
                    for segment in elements_section.entries_mut() {
                        for func_index in segment.members_mut() {
                            if *func_index >= gr_gas_available_index {
                                *func_index += 1
                            }
                        }
                    }
                }
                Section::Start(start_idx) => {
                    if *start_idx >= gr_gas_available_index {
                        *start_idx += 1;
                    }
                }
                _ => {}
            }
        }
    }

    module
}

pub(crate) fn hash_with_value_to_vec(hash_with_value: &HashWithValue) -> Vec<u8> {
    let address_data_size = mem::size_of::<HashWithValue>();
    let address_data_slice = unsafe {
        // # Safety:
        // The `unsafe` block constructs raw bytes vector of an existing rust struct
        // received by reference.
        slice::from_raw_parts(
            hash_with_value as *const HashWithValue as *const u8,
            address_data_size,
        )
    };

    address_data_slice.to_vec()
}
