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

use gear_utils::NonEmpty;
use gear_wasm_instrument::{
    parity_wasm::{
        builder,
        elements::{
            BlockType, External, FuncBody, ImportCountType, Instruction, Instructions, Internal,
            Module, Section, Type, ValueType,
        },
    },
    syscalls::SyscallName,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    iter, mem,
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

        // store available gas pointer on the first memory page
        const GAS_PTR: i32 = 1024;

        // create the body of the gas limiter:
        let mut body = Vec::with_capacity(results.len() + 9);
        body.extend_from_slice(&[
            // gr_gas_available(GAS_PTR)
            Instruction::I32Const(GAS_PTR),
            Instruction::Call(gr_gas_available_index),
            // gas_available = *GAS_PTR
            Instruction::I32Const(GAS_PTR),
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

/// Bytes data converted into wasm words, i.e. i32 words.
///
/// This type is mainly used to define values for syscalls
/// params of a pointer type. The value is converted first
/// to bytes and then to wasm words, which are later translated
/// to wasm instructions (see [`translate_ptr_data`]).
#[derive(Default)]
pub(crate) struct WasmWords(Vec<i32>);

impl WasmWords {
    const WASM_WORD_SIZE: usize = size_of::<i32>();

    pub(crate) fn new(data: impl AsRef<[u8]>) -> Self {
        let data = data.as_ref();
        let data_size = data.len();

        if data_size % Self::WASM_WORD_SIZE != 0 {
            panic!("data size isn't multiply of wasm word size")
        }

        let words = data
            .chunks_exact(Self::WASM_WORD_SIZE)
            .map(|word_bytes| {
                i32::from_le_bytes(
                    word_bytes
                        .try_into()
                        .expect("Chunks are of the exact size."),
                )
            })
            .collect();

        Self(words)
    }
}

/// Translates ptr data wasm words to instructions that set this data
/// to wasm memory.
///
/// The `start_offset` is the index in memory where data should start.
///
/// The `end_offset` is usually the same as `start_offset` when the translated
/// data (words) is desired to be used as a param for the syscall. In this case
/// end offset just points to the start of the param value.
pub(crate) fn translate_ptr_data(
    WasmWords(words): WasmWords,
    (start_offset, end_offset): (i32, Option<i32>),
) -> Vec<Instruction> {
    words
        .into_iter()
        .enumerate()
        .flat_map(|(word_idx, word)| {
            vec![
                Instruction::I32Const(start_offset),
                Instruction::I32Const(word),
                Instruction::I32Store(2, (word_idx * size_of::<i32>()) as u32),
            ]
        })
        .chain(
            end_offset
                .into_iter()
                .flat_map(|end_offset| iter::once(Instruction::I32Const(end_offset))),
        )
        .collect()
}

pub(crate) trait MemcpyUnit: Sized {
    fn load(offset: u32) -> Instruction;

    fn store(offset: u32) -> Instruction;
}

impl MemcpyUnit for u32 {
    fn load(offset: u32) -> Instruction {
        Instruction::I32Load(2, offset)
    }

    fn store(offset: u32) -> Instruction {
        Instruction::I32Store(2, offset)
    }
}

impl MemcpyUnit for u64 {
    fn load(offset: u32) -> Instruction {
        Instruction::I64Load(3, offset)
    }

    fn store(offset: u32) -> Instruction {
        Instruction::I64Store(3, offset)
    }
}

/// Creates instructions that copy N bits from the source pointer to the
/// destination pointer.
pub(crate) fn memcpy<U: MemcpyUnit>(
    dest: &[Instruction],
    src: &[Instruction],
    count: usize,
) -> Vec<Instruction> {
    memcpy_with_offsets::<U>(dest, 0, src, 0, count)
}

/// Creates instructions that copy N bits from the source pointer to the
/// destination pointer, starting at the specified offsets.
pub(crate) fn memcpy_with_offsets<U: MemcpyUnit>(
    dest: &[Instruction],
    dest_offset: usize,
    src: &[Instruction],
    src_offset: usize,
    count: usize,
) -> Vec<Instruction> {
    (0..count)
        .flat_map(|word_idx| {
            let word_offset = word_idx * size_of::<U>();
            let mut ret_instr = Vec::with_capacity(dest.len() + src.len() + 2);
            ret_instr.extend_from_slice(dest);
            ret_instr.extend_from_slice(src);
            ret_instr.extend_from_slice(&[
                U::load((src_offset + word_offset) as u32),
                U::store((dest_offset + word_offset) as u32),
            ]);
            ret_instr
        })
        .collect()
}

/// Convert `NonEmpty` vector to `Vec`.
pub(crate) fn non_empty_to_vec<T>(non_empty: NonEmpty<T>) -> Vec<T> {
    let (head, mut tail) = non_empty.into();
    tail.push(head);

    tail
}
