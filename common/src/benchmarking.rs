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

use super::*;

use gear_core::memory::{PageU32Size, WasmPage};
use gear_wasm_instrument::parity_wasm::{self, elements::*};
use sp_io::hashing::blake2_256;
use sp_runtime::traits::Zero;
use sp_std::borrow::ToOwned;

pub fn account<AccountId: Origin>(name: &'static str, index: u32, seed: u32) -> AccountId {
    let entropy = (name, index, seed).using_encoded(blake2_256);
    AccountId::from_origin(H256::from_slice(&entropy[..]))
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
            External::Memory(MemoryType::new(num_pages.raw(), None)),
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

pub fn generate_wasm(num_pages: WasmPage) -> Result<Vec<u8>, &'static str> {
    let module = create_module(num_pages);
    let code = parity_wasm::serialize(module).map_err(|_| "Failed to serialize module")?;

    Ok(code)
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
pub fn generate_wasm2(num_pages: WasmPage) -> Result<Vec<u8>, &'static str> {
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
                    Instruction::I32Const(num_pages.raw() as i32),
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

pub fn generate_wasm3(payload: Vec<u8>) -> Result<Vec<u8>, &'static str> {
    let mut module = create_module(1.into());
    module
        .insert_section(Section::Custom(CustomSection::new(
            "zeroed_section".to_owned(),
            payload,
        )))
        .unwrap();
    let code = parity_wasm::serialize(module).map_err(|_| "Failed to serialize module")?;

    Ok(code)
}

pub fn set_program<
    ProgramStorage: super::ProgramStorage<BlockNumber = BlockNumber>,
    BlockNumber: Zero,
>(
    program_id: ProgramId,
    code: Vec<u8>,
    static_pages: WasmPage,
) {
    let code_id = CodeId::generate(&code).into_origin();
    let allocations: BTreeSet<WasmPage> = static_pages.iter_from_zero().collect();
    let persistent_pages_data: BTreeMap<GearPage, PageBuf> = allocations
        .iter()
        .flat_map(|p| p.to_pages_iter())
        .map(|p| (p, PageBuf::new_zeroed()))
        .collect();
    let pages_with_data = persistent_pages_data.keys().copied().collect();

    for (page, page_buf) in persistent_pages_data {
        ProgramStorage::set_program_page_data(program_id, page, page_buf);
    }

    ProgramStorage::add_program(
        program_id,
        ActiveProgram {
            allocations,
            pages_with_data,
            code_hash: code_id,
            code_exports: Default::default(),
            static_pages,
            state: ProgramState::Initialized,
            gas_reservation_map: GasReservationMap::default(),
        },
        Zero::zero(),
    )
    .expect("benchmarking; program duplicates should not exist");
}
