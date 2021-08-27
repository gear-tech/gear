// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! Gear pallet benchmarking

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use codec::Encode;
use common::{IntermediateMessage, Origin};
use parity_wasm::elements::*;
use sp_core::H256;
use sp_io::hashing::blake2_256;
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::prelude::*;

#[allow(unused)]
use crate::Pallet as Gear;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::traits::Currency;
use frame_system::RawOrigin;

const MAX_CODE_LEN: u32 = 128 * 1024;
const MAX_PAYLOAD_LEN: u32 = 64 * 1024;
const MAX_PAGES: u32 = 512;

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
fn generate_wasm(num_pages: u32) -> Result<Vec<u8>, &'static str> {
    let module = parity_wasm::elements::Module::new(vec![
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
    ]);
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
//       (call $alloc (i32.const $num_pages))
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

    Ok(code)
}

fn set_program(program_id: H256, code: Vec<u8>, static_pages: u32, nonce: u64) {
    let code_hash = sp_io::hashing::blake2_256(&code).into();
    common::set_code(code_hash, &code);
    common::set_program(
        program_id,
        common::Program {
            static_pages,
            persistent_pages: Default::default(),
            code_hash,
            nonce,
        },
    );
}

benchmarks! {
    where_clause { where
        T::AccountId: Origin,
    }

    submit_program {
        let c in 0 .. MAX_CODE_LEN;
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller: T::AccountId = account("caller", 0, 0);
        T::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let code = vec![0u8; c as usize];
        let salt = vec![255u8; 32];
        let payload = vec![1_u8; p as usize];
        // Using a non-zero `value` to count in the transfer, as well
        let value = 10_000_u32;
    }: _(RawOrigin::Signed(caller), code, salt, payload, 100_000_000_u64, value.into())
    verify {
        assert!(Gear::<T>::message_queue().is_some());
    }

    send_message {
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller = account("caller", 0, 0);
        T::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let program_id = account::<T::AccountId>("program", 0, 100).into_origin();
        let payload = vec![0_u8; p as usize];
    }: _(RawOrigin::Signed(caller), program_id, payload, 100_000_000_u64, 10_000_u32.into())
    verify {
        assert!(Gear::<T>::message_queue().is_some());
    }

    send_reply {
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller = account("caller", 0, 0);
        T::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let program_id = account::<T::AccountId>("program", 0, 100).into_origin();
        let original_message_id = account::<T::AccountId>("message", 0, 100).into_origin();
        Gear::<T>::insert_to_mailbox(
            caller.clone().into_origin(),
            common::Message {
                id: original_message_id.clone(),
                source: program_id.clone(),
                dest: caller.clone().into_origin(),
                payload: vec![],
                gas_limit: 10_000_000_u64,
                value: 0_u128,
                reply: None,
            },
        );
        let payload = vec![0_u8; p as usize];
    }: _(RawOrigin::Signed(caller), original_message_id, payload, 100_000_000_u64, 10_000_u32.into())
    verify {
        assert!(Gear::<T>::message_queue().is_some());
    }

    initial_allocation {
        let q in 0 .. MAX_PAGES;
        let caller: T::AccountId = account("caller", 0, 0);
        T::Currency::deposit_creating(&caller, (1_u128 << 60).unique_saturated_into());
        let code = generate_wasm(q).unwrap();
        MessageQueue::<T>::append(
            IntermediateMessage::InitProgram {
                origin: caller.clone().into_origin(),
                code: code.clone(),
                program_id: account::<T::AccountId>("program", q, 0).into_origin(),
                init_message_id: account::<T::AccountId>("message", q, 100).into_origin(),
                payload: Vec::new(),
                gas_limit: 10_000_000_u64,
                value: 0,
            }
        );
    }: process_queue(RawOrigin::None)
    verify {
        assert!(Gear::<T>::message_queue().is_none());
    }

    alloc_in_handle {
        let q in 0 .. MAX_PAGES;
        let caller: T::AccountId = account("caller", 0, 0);
        T::Currency::deposit_creating(&caller, (1_u128 << 60).unique_saturated_into());
        let code = generate_wasm2(q as i32).unwrap();
        let program_id = account::<T::AccountId>("program", q, 0).into_origin();
        set_program(program_id, code, 1_u32, q as u64);
        MessageQueue::<T>::append(
            IntermediateMessage::DispatchMessage {
                id: account::<T::AccountId>("message", q, 100).into_origin(),
                origin: caller.into_origin(),
                destination: program_id,
                payload: vec![],
                gas_limit: 10_000_000_u64,
                value: 0,
                reply: None,
            }
        );
    }: process_queue(RawOrigin::None)
    verify {
        assert!(Gear::<T>::message_queue().is_none());
    }
}

impl_benchmark_test_suite!(Gear, crate::mock::new_test_ext(), crate::mock::Test);
