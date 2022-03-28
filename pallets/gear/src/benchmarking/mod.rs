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

//! Benchmarks for the contracts pallet

#![cfg(feature = "runtime-benchmarks")]

mod code;
mod sandbox;

use self::{
    code::{
        body::{self, DynInstr::*},
        DataSegment, ImportedFunction, ImportedMemory, Location, ModuleDefinition, WasmModule,
    },
    sandbox::Sandbox,
};
use crate::{schedule::INSTR_BENCHMARK_BATCH_SIZE, Pallet as Contracts, *};
use codec::{Encode, HasCompact, MaxEncodedLen};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::Get;
use frame_support::weights::Weight;
use frame_system::RawOrigin;
use scale_info::TypeInfo;
use sp_runtime::{
    app_crypto::UncheckedFrom,
    traits::{Bounded, Hash},
    Perbill,
};
use sp_std::prelude::*;
use wasm_instrument::parity_wasm::elements::{BlockType, BrTableData, Instruction, ValueType};

use common::Origin;
use gear_core::program::CodeHash;
use parity_wasm::elements::*;
use sp_core::H256;
use sp_io::hashing::blake2_256;
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::borrow::ToOwned;
use sp_std::prelude::*;

#[allow(unused)]
use crate::Pallet as Gear;
use frame_support::traits::Currency;

const MIN_CODE_LEN: u32 = 128;
const MAX_CODE_LEN: u32 = 128 * 1024;
const MAX_PAYLOAD_LEN: u32 = 64 * 1024;
const MAX_PAGES: u32 = 512;

/// How many batches we do per API benchmark.
const API_BENCHMARK_BATCHES: u32 = 20;

/// How many batches we do per Instruction benchmark.
const INSTR_BENCHMARK_BATCHES: u32 = 50;

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
fn create_module(num_pages: u32) -> parity_wasm::elements::Module {
    parity_wasm::elements::Module::new(vec![
        Section::Type(TypeSection::with_types(vec![Type::Function(
            FunctionType::new(vec![], vec![]),
        )])),
        Section::Import(ImportSection::with_entries(vec![ImportEntry::new(
            "env".into(),
            "memory".into(),
            External::Memory(MemoryType::new(num_pages, None)),
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

fn generate_wasm(num_pages: u32) -> Result<Vec<u8>, &'static str> {
    let module = create_module(num_pages);
    let code = parity_wasm::serialize(module).map_err(|_| "Failed to serialize module")?;
    log::debug!("wasm");

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
fn generate_wasm2(num_pages: i32) -> Result<Vec<u8>, &'static str> {
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
                    Instruction::I32Const(num_pages),
                    Instruction::Call(0),
                    Instruction::SetLocal(0),
                    Instruction::End,
                ]),
            ),
        ])),
    ]);
    let code = parity_wasm::serialize(module).map_err(|_| "Failed to serialize module")?;
    log::debug!("wasm2");

    Ok(code)
}

fn generate_wasm3(payload: Vec<u8>) -> Result<Vec<u8>, &'static str> {
    let mut module = create_module(1);
    module
        .insert_section(Section::Custom(CustomSection::new(
            "zeroed_section".to_owned(),
            payload,
        )))
        .unwrap();
    let code = parity_wasm::serialize(module).map_err(|_| "Failed to serialize module")?;
    log::debug!("wasm3");

    Ok(code)
}

fn set_program(program_id: H256, code: Vec<u8>, static_pages: u32, nonce: u64) {
    let code_hash = CodeHash::generate(&code).into_origin();
    common::set_program(
        program_id,
        common::ActiveProgram {
            static_pages,
            persistent_pages: (0..static_pages).collect(),
            code_hash,
            nonce,
            state: common::ProgramState::Initialized,
        },
        (0..static_pages).map(|i| (i, vec![0u8; 65536])).collect(),
    );
}

/// An instantiated and deployed contract.
struct Contract<T: Config> {
    caller: T::AccountId,
    account_id: T::AccountId,
}

benchmarks! {

    where_clause { where
        T::AccountId: Origin,
    }

    submit_code {
        let c in 0 .. MAX_CODE_LEN;
        let caller: T::AccountId = account("caller", 0, 0);
        let code = generate_wasm3(vec![0u8; c as usize]).unwrap();
        let code_hash: H256 = CodeHash::generate(&code).into_origin();
    }: _(RawOrigin::Signed(caller), code)
    verify {
        assert!(common::code_exists(code_hash));
    }

    submit_program {
        let c in MIN_CODE_LEN .. MAX_CODE_LEN;
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller: T::AccountId = account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let code = generate_wasm3(vec![0u8; (c - MIN_CODE_LEN) as usize]).unwrap();
        let salt = vec![255u8; 32];
        let payload = vec![1_u8; p as usize];
        // Using a non-zero `value` to count in the transfer, as well
        let value = 10_000_u32;
    }: _(RawOrigin::Signed(caller), code, salt, payload, 100_000_000_u64, value.into())
    verify {
        assert!(common::dequeue_dispatch().is_some());
    }

    send_message {
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller = account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let program_id = account::<T::AccountId>("program", 0, 100).into_origin();
        let code = generate_wasm2(16_i32).unwrap();
        set_program(program_id, code, 1_u32, 0_u64);
        let payload = vec![0_u8; p as usize];
    }: _(RawOrigin::Signed(caller), program_id, payload, 100_000_000_u64, 10_000_u32.into())
    verify {
        assert!(common::dequeue_dispatch().is_some());
    }

    send_reply {
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller = account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let program_id = account::<T::AccountId>("program", 0, 100).into_origin();
        let code = generate_wasm2(16_i32).unwrap();
        set_program(program_id, code, 1_u32, 0_u64);
        let original_message_id = account::<T::AccountId>("message", 0, 100).into_origin();
        Gear::<T>::insert_to_mailbox(
            caller.clone().into_origin(),
            common::QueuedMessage {
                id: original_message_id,
                source: program_id,
                dest: caller.clone().into_origin(),
                payload: vec![],
                value: 0_u128,
                reply: None,
            },
        );
        let payload = vec![0_u8; p as usize];
    }: _(RawOrigin::Signed(caller), original_message_id, payload, 100_000_000_u64, 10_000_u32.into())
    verify {
        assert!(common::dequeue_dispatch().is_some());
    }

    initial_allocation {
        let q in 1 .. MAX_PAGES;
        let caller: T::AccountId = account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, (1u128 << 60).unique_saturated_into());
        let code = generate_wasm(q).unwrap();
        let salt = vec![255u8; 32];
    }: {
        let _ = crate::Pallet::<T>::submit_program(RawOrigin::Signed(caller).into(), code, salt, vec![], 100_000_000u64, 0u32.into());
        crate::Pallet::<T>::process_queue();
    }
    verify {
        assert!(common::dequeue_dispatch().is_none());
    }

    alloc_in_handle {
        let q in 0 .. MAX_PAGES;
        let caller: T::AccountId = account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, (1_u128 << 60).unique_saturated_into());
        let code = generate_wasm2(q as i32).unwrap();
        let salt = vec![255u8; 32];
    }: {
        let _ = crate::Pallet::<T>::submit_program(RawOrigin::Signed(caller).into(), code, salt, vec![], 100_000_000u64, 0u32.into());
        crate::Pallet::<T>::process_queue();
    }
    verify {
        assert!(common::dequeue_dispatch().is_none());
    }

    // We make the assumption that pushing a constant and dropping a value takes roughly
    // the same amount of time. We follow that `t.load` and `drop` both have the weight
    // of this benchmark / 2. We need to make this assumption because there is no way
    // to measure them on their own using a valid wasm module. We need their individual
    // values to derive the weight of individual instructions (by substraction) from
    // benchmarks that include those for parameter pushing and return type dropping.
    // We call the weight of `t.load` and `drop`: `w_param`.
    // The weight that would result from the respective benchmark we call: `w_bench`.
    //
    // w_i{32,64}const = w_drop = w_bench / 2
    instr_i64const {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_i{32,64}load = w_bench - 2 * w_param
    instr_i64load {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomUnaligned(0, code::max_pages::<T>() * 64 * 1024 - 8),
                Regular(Instruction::I64Load(3, 0)),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_i{32,64}store{...} = w_bench - 2 * w_param
    instr_i64store {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomUnaligned(0, code::max_pages::<T>() * 64 * 1024 - 8),
                RandomI64Repeated(1),
                Regular(Instruction::I64Store(3, 0)),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_select = w_bench - 4 * w_param
    instr_select {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI64Repeated(1),
                RandomI64Repeated(1),
                RandomI32(0, 2),
                Regular(Instruction::Select),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_if = w_bench - 3 * w_param
    instr_if {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI32(0, 2),
                Regular(Instruction::If(BlockType::Value(ValueType::I64))),
                RandomI64Repeated(1),
                Regular(Instruction::Else),
                RandomI64Repeated(1),
                Regular(Instruction::End),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_br = w_bench - 2 * w_param
    // Block instructions are not counted.
    instr_br {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                Regular(Instruction::Block(BlockType::NoResult)),
                Regular(Instruction::Block(BlockType::NoResult)),
                Regular(Instruction::Block(BlockType::NoResult)),
                Regular(Instruction::Br(1)),
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
                Regular(Instruction::End),
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
                Regular(Instruction::End),
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
                Regular(Instruction::End),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_br_if = w_bench - 3 * w_param
    // Block instructions are not counted.
    instr_br_if {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                Regular(Instruction::Block(BlockType::NoResult)),
                Regular(Instruction::Block(BlockType::NoResult)),
                Regular(Instruction::Block(BlockType::NoResult)),
                Regular(Instruction::I32Const(1)),
                Regular(Instruction::BrIf(1)),
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
                Regular(Instruction::End),
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
                Regular(Instruction::End),
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
                Regular(Instruction::End),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_br_table = w_bench - 3 * w_param
    // Block instructions are not counted.
    instr_br_table {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let table = Box::new(BrTableData {
            table: Box::new([1, 1, 1]),
            default: 1,
        });
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                Regular(Instruction::Block(BlockType::NoResult)),
                Regular(Instruction::Block(BlockType::NoResult)),
                Regular(Instruction::Block(BlockType::NoResult)),
                RandomI32(0, 4),
                Regular(Instruction::BrTable(table)),
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
                Regular(Instruction::End),
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
                Regular(Instruction::End),
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
                Regular(Instruction::End),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_br_table_per_entry = w_bench
    instr_br_table_per_entry {
        let e in 1 .. T::Schedule::get().limits.br_table_size;
        let entry: Vec<u32> = [0, 1].iter()
            .cloned()
            .cycle()
            .take((e / 2) as usize).collect();
        let table = Box::new(BrTableData {
            table: entry.into_boxed_slice(),
            default: 0,
        });
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(body::repeated_dyn(INSTR_BENCHMARK_BATCH_SIZE, vec![
                Regular(Instruction::Block(BlockType::NoResult)),
                Regular(Instruction::Block(BlockType::NoResult)),
                Regular(Instruction::Block(BlockType::NoResult)),
                RandomI32(0, (e + 1) as i32), // Make sure the default entry is also used
                Regular(Instruction::BrTable(table)),
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
                Regular(Instruction::End),
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
                Regular(Instruction::End),
                RandomI64Repeated(1),
                Regular(Instruction::Drop),
                Regular(Instruction::End),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_call = w_bench - 2 * w_param
    instr_call {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            // We need to make use of the stack here in order to trigger stack height
            // instrumentation.
            aux_body: Some(body::plain(vec![
                Instruction::I64Const(42),
                Instruction::Drop,
                Instruction::End,
            ])),
            call_body: Some(body::repeated(r * INSTR_BENCHMARK_BATCH_SIZE, &[
                Instruction::Call(2), // call aux
            ])),
            inject_stack_metering: true,
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_call_indrect = w_bench - 3 * w_param
    instr_call_indirect {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let num_elements = T::Schedule::get().limits.table_size;
        use self::code::TableSegment;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            // We need to make use of the stack here in order to trigger stack height
            // instrumentation.
            aux_body: Some(body::plain(vec![
                Instruction::I64Const(42),
                Instruction::Drop,
                Instruction::End,
            ])),
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI32(0, num_elements as i32),
                Regular(Instruction::CallIndirect(0, 0)), // we only have one sig: 0
            ])),
            inject_stack_metering: true,
            table: Some(TableSegment {
                num_elements,
                function_index: 2, // aux
            }),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_instr_call_indirect_per_param = w_bench - 1 * w_param
    // Calling a function indirectly causes it to go through a thunk function whose runtime
    // linearly depend on the amount of parameters to this function.
    // Please note that this is not necessary with a direct call.
    instr_call_indirect_per_param {
        let p in 0 .. T::Schedule::get().limits.parameters;
        let num_elements = T::Schedule::get().limits.table_size;
        use self::code::TableSegment;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            // We need to make use of the stack here in order to trigger stack height
            // instrumentation.
            aux_body: Some(body::plain(vec![
                Instruction::I64Const(42),
                Instruction::Drop,
                Instruction::End,
            ])),
            aux_arg_num: p,
            call_body: Some(body::repeated_dyn(INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI64Repeated(p as usize),
                RandomI32(0, num_elements as i32),
                Regular(Instruction::CallIndirect(p.min(1), 0)), // aux signature: 1 or 0
            ])),
            inject_stack_metering: true,
            table: Some(TableSegment {
                num_elements,
                function_index: 2, // aux
            }),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_local_get = w_bench - 1 * w_param
    instr_local_get {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let max_locals = T::Schedule::get().limits.stack_height;
        let mut call_body = body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
            RandomGetLocal(0, max_locals),
            Regular(Instruction::Drop),
        ]);
        body::inject_locals(&mut call_body, max_locals);
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(call_body),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_local_set = w_bench - 1 * w_param
    instr_local_set {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let max_locals = T::Schedule::get().limits.stack_height;
        let mut call_body = body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
            RandomI64Repeated(1),
            RandomSetLocal(0, max_locals),
        ]);
        body::inject_locals(&mut call_body, max_locals);
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(call_body),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_local_tee = w_bench - 2 * w_param
    instr_local_tee {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let max_locals = T::Schedule::get().limits.stack_height;
        let mut call_body = body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
            RandomI64Repeated(1),
            RandomTeeLocal(0, max_locals),
            Regular(Instruction::Drop),
        ]);
        body::inject_locals(&mut call_body, max_locals);
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(call_body),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_global_get = w_bench - 1 * w_param
    instr_global_get {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let max_globals = T::Schedule::get().limits.globals;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomGetGlobal(0, max_globals),
                Regular(Instruction::Drop),
            ])),
            num_globals: max_globals,
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_global_set = w_bench - 1 * w_param
    instr_global_set {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let max_globals = T::Schedule::get().limits.globals;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI64Repeated(1),
                RandomSetGlobal(0, max_globals),
            ])),
            num_globals: max_globals,
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_memory_get = w_bench - 1 * w_param
    instr_memory_current {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            call_body: Some(body::repeated(r * INSTR_BENCHMARK_BATCH_SIZE, &[
                Instruction::CurrentMemory(0),
                Instruction::Drop
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_memory_grow = w_bench - 2 * w_param
    // We can only allow allocate as much memory as it is allowed in a a contract.
    // Therefore the repeat count is limited by the maximum memory any contract can have.
    // Using a contract with more memory will skew the benchmark because the runtime of grow
    // depends on how much memory is already allocated.
    instr_memory_grow {
        let r in 0 .. 1;
        let max_pages = ImportedMemory::max::<T>().max_pages;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory {
                min_pages: 0,
                max_pages,
            }),
            call_body: Some(body::repeated(r * max_pages, &[
                Instruction::I32Const(1),
                Instruction::GrowMemory(0),
                Instruction::Drop,
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // Unary numeric instructions.
    // All use w = w_bench - 2 * w_param.

    instr_i64clz {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr(
            Instruction::I64Clz,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64ctz {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr(
            Instruction::I64Ctz,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64popcnt {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr(
            Instruction::I64Popcnt,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64eqz {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr(
            Instruction::I64Eqz,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64extendsi32 {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI32Repeated(1),
                Regular(Instruction::I64ExtendSI32),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    instr_i64extendui32 {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            call_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI32Repeated(1),
                Regular(Instruction::I64ExtendUI32),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    instr_i32wrapi64 {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr(
            Instruction::I32WrapI64,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    // Binary numeric instructions.
    // All use w = w_bench - 3 * w_param.

    instr_i64eq {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64Eq,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64ne {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64Ne,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64lts {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64LtS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64ltu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64LtU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64gts {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64GtS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64gtu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64GtU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64les {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64LeS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64leu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64LeU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64ges {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64GeS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64geu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64GeU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64add {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64Add,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64sub {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64Sub,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64mul {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64Mul,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64divs {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64DivS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64divu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64DivU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64rems {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64RemS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64remu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64RemU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64and {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64And,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64or {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64Or,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64xor {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64Xor,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64shl {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64Shl,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64shrs {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64ShrS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64shru {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64ShrU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64rotl {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64Rotl,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64rotr {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr(
            Instruction::I64Rotr,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    impl_benchmark_test_suite!(
        Gear,
        crate::tests::ExtBuilder::default().build(),
        crate::tests::Test,
    )
}
