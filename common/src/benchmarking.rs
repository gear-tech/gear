// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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
use gear_core::{
    pages::WasmPage,
    program::{MemoryInfix, ProgramState},
    reservation::GasReservationMap,
};
use gear_wasm_instrument::{
    Export, FuncType, Function, Import, Instruction, Module, ModuleBuilder, ValType,
};
use sp_io::hashing::blake2_256;
use sp_runtime::traits::Zero;

pub fn account<AccountId: Origin>(name: &'static str, index: u32, seed: u32) -> AccountId {
    let entropy = (name, index, seed).using_encoded(blake2_256);
    H256::from_slice(&entropy[..]).cast()
}

// A wasm module that allocates `$num_pages` of memory in `init` function.
// In text format would look something like
// (module
//     (type (func))
//     (import "env" "memory" (memory $num_pages))
//     (func (type 0))
//     (export "init" (func 0)))
pub fn create_module(num_pages: WasmPage) -> Module {
    let mut mbuilder = ModuleBuilder::default();
    mbuilder.push_import(Import::memory(num_pages.into(), None));
    mbuilder.add_func(FuncType::new([], []), Function::default());
    mbuilder.push_export(Export::func("init", 0));
    mbuilder.build()
}

// A wasm module that allocates `$num_pages` in `handle` function:
// (module
//     (import "env" "memory" (memory 1))
//     (import "env" "alloc" (func $alloc (param i32) (result i32)))
//     (export "init" (func $init))
//     (export "handle" (func $handle))
//     (func $init)
//     (func $handle
//         (local $result i32)
//         (local.set $result (call $alloc (i32.const $num_pages)))
//     )
// )
pub fn generate_wasm(num_pages: WasmPage) -> Result<Vec<u8>, &'static str> {
    let mut mbuilder = ModuleBuilder::default();
    mbuilder.push_import(Import::memory(num_pages.into(), None));

    // alloc
    let alloc_idx = mbuilder.push_type(FuncType::new([ValType::I32], [ValType::I32]));
    mbuilder.push_import(Import::func("env", "alloc", alloc_idx));

    // init
    let init_idx = mbuilder.add_func(FuncType::new([], []), Function::default());
    mbuilder.push_export(Export::func("init", init_idx));

    // handle
    let handle_idx = mbuilder.add_func(
        FuncType::new([], []),
        Function {
            locals: vec![(1, ValType::I32)],
            instructions: vec![
                Instruction::I32Const(u32::from(num_pages) as i32),
                Instruction::Call(0),
                Instruction::LocalSet(0),
                Instruction::End,
            ],
        },
    );
    mbuilder.push_export(Export::func("handle", handle_idx));

    let code = mbuilder
        .build()
        .serialize()
        .map_err(|_| "Failed to serialize module")?;

    Ok(code)
}

pub fn set_program<ProgramStorage, BlockNumber>(program_id: ActorId, code: Vec<u8>)
where
    ProgramStorage: super::ProgramStorage<BlockNumber = BlockNumber>,
    BlockNumber: Zero + Copy + Saturating,
{
    ProgramStorage::add_program(
        program_id,
        ActiveProgram {
            allocations_tree_len: 0,
            code_id: CodeId::generate(&code),
            state: ProgramState::Initialized,
            gas_reservation_map: GasReservationMap::default(),
            expiration_block: Zero::zero(),
            memory_infix: MemoryInfix::new(1u32),
        },
    )
    .expect("benchmarking; program duplicates should not exist");
}
