// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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
    ids::prelude::*,
    pages::{WasmPage, WasmPagesAmount},
    program::{MemoryInfix, ProgramState},
    reservation::GasReservationMap,
};
use gear_wasm_instrument::parity_wasm::{self, elements::*};
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
pub fn create_module(num_pages: WasmPage) -> parity_wasm::elements::Module {
    parity_wasm::elements::Module::new(vec![
        Section::Type(TypeSection::with_types(vec![Type::Function(
            FunctionType::new(vec![], vec![]),
        )])),
        Section::Import(ImportSection::with_entries(vec![ImportEntry::new(
            "env".into(),
            "memory".into(),
            External::Memory(MemoryType::new(num_pages.into(), None)),
        )])),
        Section::Function(FunctionSection::with_entries(vec![Func::new(0)])),
        Section::Export(ExportSection::with_entries(vec![ExportEntry::new(
            "init".into(),
            Internal::Function(0),
        )])),
        Section::Code(CodeSection::with_bodies(vec![FuncBody::new(
            vec![],
            Instructions::new(vec![Instruction::End]),
        )])),
    ])
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
    let module = parity_wasm::elements::Module::new(vec![
        Section::Type(TypeSection::with_types(vec![
            Type::Function(FunctionType::new(
                vec![ValueType::I32],
                vec![ValueType::I32],
            )),
            Type::Function(FunctionType::new(vec![], vec![])),
        ])),
        Section::Import(ImportSection::with_entries(vec![
            ImportEntry::new(
                "env".into(),
                "memory".into(),
                External::Memory(MemoryType::new(1_u32, None)),
            ),
            ImportEntry::new("env".into(), "alloc".into(), External::Function(0_u32)),
        ])),
        Section::Function(FunctionSection::with_entries(vec![
            Func::new(1_u32),
            Func::new(1_u32),
        ])),
        Section::Export(ExportSection::with_entries(vec![
            ExportEntry::new("init".into(), Internal::Function(1)),
            ExportEntry::new("handle".into(), Internal::Function(2)),
        ])),
        Section::Code(CodeSection::with_bodies(vec![
            FuncBody::new(vec![], Instructions::new(vec![Instruction::End])),
            FuncBody::new(
                vec![Local::new(1, ValueType::I32)],
                Instructions::new(vec![
                    Instruction::I32Const(u32::from(num_pages) as i32),
                    Instruction::Call(0),
                    Instruction::SetLocal(0),
                    Instruction::End,
                ]),
            ),
        ])),
    ]);
    let code = parity_wasm::serialize(module).map_err(|_| "Failed to serialize module")?;

    Ok(code)
}

pub fn set_program<ProgramStorage, BlockNumber>(
    program_id: ProgramId,
    code: Vec<u8>,
    static_pages: WasmPagesAmount,
) where
    ProgramStorage: super::ProgramStorage<BlockNumber = BlockNumber>,
    BlockNumber: Zero + Copy + Saturating,
{
    ProgramStorage::add_program(
        program_id,
        ActiveProgram {
            allocations_tree_len: 0,
            code_hash: CodeId::generate(&code).into_origin(),
            code_exports: Default::default(),
            static_pages,
            state: ProgramState::Initialized,
            gas_reservation_map: GasReservationMap::default(),
            expiration_block: Zero::zero(),
            memory_infix: MemoryInfix::new(1u32),
        },
    )
    .expect("benchmarking; program duplicates should not exist");
}
