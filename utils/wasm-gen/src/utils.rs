// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use gear_wasm_instrument::parity_wasm::elements::{FuncBody, ImportCountType, Instruction, Module};
use std::collections::HashMap;

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

    let mut colored_list = Vec::<HashMap<_, _>>::with_capacity(function_bodies.len());
    const PREALLOCATE: usize = 1_000;
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
    colored: &mut HashMap<usize, Color>,
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
            callback(&path, called_index);
        }
    }

    colored.insert(call_index, Color::Black);
    path.pop();
}
