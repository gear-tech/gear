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

use gear_wasm_instrument::parity_wasm::{
    builder,
    elements::{self, FuncBody, ImportCountType, Instruction, Module, Type, ValueType},
};
use std::collections::{BTreeMap, BTreeSet};

const PREALLOCATE: usize = 1_000;

enum Color {
    Grey,
    Black,
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
                    .with_instructions(elements::Instructions::new(body))
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
