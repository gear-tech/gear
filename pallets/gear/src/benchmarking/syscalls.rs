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

//! Benchmarks for gear sys-calls.

use super::{
    code::{
        body::{self, DynInstr::*},
        max_pages, DataSegment, ImportedMemory, ModuleDefinition, WasmModule,
    },
    utils::{self, PrepareConfig},
    Exec, Program,
};
use crate::{
    benchmarking::MAX_PAYLOAD_LEN, manager::HandleKind, schedule::API_BENCHMARK_BATCH_SIZE, Config,
    MailboxOf, Pallet as Gear, ProgramStorageOf,
};
use alloc::{vec, vec::Vec};
use codec::Encode;
use common::{benchmarking, storage::*, Origin, ProgramStorage};
use core::{marker::PhantomData, mem, mem::size_of, ops::Range};
use frame_system::RawOrigin;
use gear_core::{
    ids::{MessageId, ProgramId, ReservationId},
    memory::{GearPage, PageBuf, PageBufInner, PageU32Size, WasmPage},
    message::Message,
    reservation::GasReservationSlot,
};
use gear_wasm_instrument::{parity_wasm::elements::Instruction, syscalls::SysCallName};
use sp_core::Get;
use sp_runtime::traits::UniqueSaturatedInto;

pub(crate) struct Benches<T>
where
    T: Config,
    T::AccountId: Origin,
{
    _phantom: PhantomData<T>,
}

impl<T> Benches<T>
where
    T: Config,
    T::AccountId: Origin,
{
    // size of error length field
    const ERR_LEN_SIZE: u32 = size_of::<u32>() as u32;
    const GR_DEBUG_STRING_LEN: u32 = 100;
    const GR_READ_BUFFER_LEN: u32 = 100;
    const MAX_PAGES: u16 = 64;

    fn prepare_handle(
        code: WasmModule<T>,
        value: u32,
        err_len_ptrs: Range<u32>,
    ) -> Result<Exec<T>, &'static str> {
        let instance = Program::<T>::new(code, vec![])?;

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            err_len_ptrs,
            PrepareConfig {
                value: value.into(),
                ..Default::default()
            },
        )
    }

    const fn err_len_ptrs(repetitions: u32, offset: u32) -> Range<u32> {
        offset..repetitions * Self::ERR_LEN_SIZE + offset
    }

    pub fn alloc(r: u32) -> Result<Exec<T>, &'static str> {
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory { min_pages: 0 }),
            imported_functions: vec![SysCallName::Alloc],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    // Alloc 0 pages take almost the same amount of resources as another amount.
                    Instruction::I32Const(0),
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, 0..0)
    }

    pub fn free(r: u32) -> Result<Exec<T>, &'static str> {
        assert!(r <= max_pages::<T>() as u32);

        use Instruction::*;
        let mut instructions = vec![];
        for _ in 0..API_BENCHMARK_BATCH_SIZE {
            instructions.push(I32Const(r as i32));
            instructions.push(Call(0));
            instructions.push(Drop);
            for page in 0..r {
                instructions.push(I32Const(page as i32));
                instructions.push(Call(1));
                instructions.push(Drop);
            }
        }
        instructions.push(End);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory { min_pages: 0 }),
            imported_functions: vec![SysCallName::Alloc, SysCallName::Free],
            init_body: None,
            handle_body: Some(body::plain(instructions)),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, 0..0)
    }

    pub fn gr_reserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
        let err_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, 1);
        let mailbox_threshold = <T as Config>::MailboxThreshold::get(); // It is not allowed to reserve less than mailbox threshold

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReserveGas],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // gas amount
                    Regular(Instruction::I64Const(mailbox_threshold as i64)),
                    // duration
                    Regular(Instruction::I32Const(1)),
                    // err_rid ptr
                    Counter(1, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, err_ptrs)
    }

    pub fn gr_unreserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
        let reservation_id_offset = 1;
        let reservation_ids = (0..r * API_BENCHMARK_BATCH_SIZE)
            .map(|i| ReservationId::from(i as u64))
            .collect::<Vec<_>>();
        let reservation_id_bytes: Vec<u8> =
            reservation_ids.iter().flat_map(|x| x.encode()).collect();

        let err_offset = reservation_id_offset + reservation_id_bytes.len() as u32;
        let err_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, err_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::UnreserveGas],
            data_segments: vec![DataSegment {
                offset: reservation_id_offset,
                value: reservation_id_bytes,
            }],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // reservation_id ptr
                    Counter(reservation_id_offset, size_of::<ReservationId>() as u32),
                    // err_unreserved ptr
                    Counter(err_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        // insert gas reservation slots
        let program_id = ProgramId::from_origin(instance.addr);
        ProgramStorageOf::<T>::update_active_program(program_id, |program, _bn| {
            for x in 0..r * API_BENCHMARK_BATCH_SIZE {
                program.gas_reservation_map.insert(
                    ReservationId::from(x as u64),
                    GasReservationSlot {
                        amount: 1_000,
                        start: 1,
                        finish: 100,
                    },
                );
            }
        })
        .unwrap();

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(program_id),
            vec![],
            err_ptrs,
            Default::default(),
        )
    }

    pub fn gr_system_reserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
        let err_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, 1);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::SystemReserveGas],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // gas amount
                    Regular(Instruction::I64Const(50_000_000)),
                    // err len ptr
                    Counter(1, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            err_ptrs,
            Default::default(),
        )
    }

    pub fn getter(name: SysCallName, r: u32) -> Result<Exec<T>, &'static str> {
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![name],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    // ptr to write taken data
                    Instruction::I32Const(1),
                    // CALL
                    Instruction::Call(0),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, 0..0)
    }

    pub fn gr_read(r: u32) -> Result<Exec<T>, &'static str> {
        let buffer_offset = 1;
        let buffer_len = Self::GR_READ_BUFFER_LEN;

        let err_len_offset = buffer_offset + buffer_len;
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, err_len_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Read],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // at
                    Regular(Instruction::I32Const(0)),
                    // len
                    Regular(Instruction::I32Const(buffer_len as i32)),
                    // buffer ptr
                    Regular(Instruction::I32Const(buffer_offset as i32)),
                    // err len ptr
                    Counter(err_len_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![0; (buffer_len + buffer_offset) as usize],
            err_len_ptrs,
            Default::default(),
        )
    }

    pub fn gr_read_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let buffer_offset = 1;
        let buffer_len = n as i32 * 1024;

        assert!(
            WasmPage::from_offset((buffer_offset + buffer_len) as u32) < Self::MAX_PAGES.into()
        );
        assert!(buffer_len <= MAX_PAYLOAD_LEN as i32);

        let instrs_batch = body::with_result_check(
            0,
            &[
                // at
                Instruction::I32Const(0),
                // len
                Instruction::I32Const(buffer_len),
                // buffer ptr
                Instruction::I32Const(buffer_offset),
                // err len ptr
                Instruction::I32Const(0),
                // CALL
                Instruction::Call(0),
            ],
        );

        // Access const amount of pages before debug call in order to remove lazy-pages factor.
        let instrs = body::write_access_all_pages_instrs(Self::MAX_PAGES.into(), vec![]);
        let instrs = body::repeated_instr(API_BENCHMARK_BATCH_SIZE, instrs_batch, instrs);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Read],
            handle_body: Some(body::from_instructions(instrs)),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![0xff; MAX_PAYLOAD_LEN as usize],
            0..0,
            Default::default(),
        )
    }

    pub fn gr_random(r: u32) -> Result<Exec<T>, &'static str> {
        let subject_ptr = 1;
        let subject_len = 32;
        let bn_random_ptr = subject_ptr + subject_len;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Random],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    // subject ptr
                    Instruction::I32Const(subject_ptr),
                    // bn_random ptr
                    Instruction::I32Const(bn_random_ptr),
                    // CALL
                    Instruction::Call(0),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, 0..0)
    }

    pub fn gr_send_init(r: u32) -> Result<Exec<T>, &'static str> {
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, 1);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::SendInit],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_handle ptr
                    Counter(1, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, err_len_ptrs)
    }

    pub fn gr_send_push(r: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = 100u32;

        let err_handle_ptr = payload_offset + payload_len;

        let err_offset = err_handle_ptr + 8;
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, err_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::SendInit, SysCallName::SendPush],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_handle ptr for send_init
                    Regular(Instruction::I32Const(err_handle_ptr as i32)),
                    // CALL init
                    Regular(Instruction::Call(0)),
                    // handle
                    Regular(Instruction::I32Const((err_handle_ptr + 4) as i32)),
                    Regular(Instruction::I32Load(2, 0)),
                    // payload ptr
                    Regular(Instruction::I32Const(payload_offset as i32)),
                    // payload len
                    Regular(Instruction::I32Const(payload_len as i32)),
                    // err len ptr
                    Counter(err_offset, Self::ERR_LEN_SIZE),
                    // CALL push
                    Regular(Instruction::Call(1)),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, err_len_ptrs)
    }

    // TODO: investigate how handle changes can affect on syscall perf (issue #1722).
    pub fn gr_send_push_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = n * 1024;

        let handle_offset = payload_offset + payload_len;

        let err_offset = handle_offset + 8; // u32 + u32 offset
        let err_len_ptrs = Self::err_len_ptrs(API_BENCHMARK_BATCH_SIZE, err_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::SendInit, SysCallName::SendPush],
            handle_body: Some(body::repeated_dyn(
                API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_handle ptr for send_init
                    Regular(Instruction::I32Const(handle_offset as i32)),
                    // CALL init
                    Regular(Instruction::Call(0)),
                    // handle
                    Regular(Instruction::I32Const((handle_offset + 4) as i32)),
                    Regular(Instruction::I32Load(2, 0)),
                    // payload ptr
                    Regular(Instruction::I32Const(payload_offset as i32)),
                    // payload len
                    Regular(Instruction::I32Const(payload_len as i32)),
                    // err len ptr
                    Counter(err_offset, Self::ERR_LEN_SIZE),
                    // CALL push
                    Regular(Instruction::Call(1)),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, err_len_ptrs)
    }

    // Benchmark the `gr_send_commit` call.
    // `gr_send` call is shortcut for `gr_send_init` + `gr_send_commit`
    pub fn gr_send_commit(r: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = 100u32;

        let pid_value_offset = payload_offset + payload_len;
        let pid_value = vec![0; 32 + 16];

        let err_mid_offset = pid_value_offset + pid_value.len() as u32;
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, err_mid_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Send],
            data_segments: vec![DataSegment {
                offset: pid_value_offset,
                value: pid_value,
            }],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // pid_value ptr
                    Regular(Instruction::I32Const(pid_value_offset as i32)),
                    // payload ptr
                    Regular(Instruction::I32Const(payload_offset as i32)),
                    // payload len
                    Regular(Instruction::I32Const(payload_len as i32)),
                    // delay
                    Regular(Instruction::I32Const(10)),
                    // err_mid ptr
                    Counter(err_mid_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 10000000, err_len_ptrs)
    }

    // Benchmark the `gr_send_commit` call.
    // `gr_send` call is shortcut for `gr_send_init` + `gr_send_commit`
    pub fn gr_send_commit_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = n * 1024;

        let pid_value_offset = payload_offset + payload_len;
        let pid_value = vec![0; 32 + 16];

        let err_mid_offset = pid_value_offset + pid_value.len() as u32;
        let err_len_ptrs = Self::err_len_ptrs(API_BENCHMARK_BATCH_SIZE, err_mid_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Send],
            data_segments: vec![DataSegment {
                offset: pid_value_offset,
                value: pid_value,
            }],
            handle_body: Some(body::repeated_dyn(
                API_BENCHMARK_BATCH_SIZE,
                vec![
                    // pid_value ptr
                    Regular(Instruction::I32Const(pid_value_offset as i32)),
                    // payload ptr
                    Regular(Instruction::I32Const(payload_offset as i32)),
                    // payload len
                    Regular(Instruction::I32Const(payload_len as i32)),
                    // delay
                    Regular(Instruction::I32Const(10)),
                    // err_mid ptr
                    Counter(err_mid_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 10000000, err_len_ptrs)
    }

    // Benchmark the `gr_reservation_send_commit` call.
    // `gr_send` call is shortcut for `gr_send_init` + `gr_send_commit`
    pub fn gr_reservation_send_commit(r: u32) -> Result<Exec<T>, &'static str> {
        let rid_pid_value_offset = 1;

        let rid_pid_values = (0..r * API_BENCHMARK_BATCH_SIZE)
            .flat_map(|i| {
                let mut bytes = [0; 80];
                bytes[..32].copy_from_slice(ReservationId::from(i as u64).as_ref());
                bytes
            })
            .collect::<Vec<_>>();

        let payload_offset = rid_pid_value_offset + rid_pid_values.len() as u32;
        let payload_len = 100;

        let err_mid_offset = payload_offset + payload_len;
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, err_mid_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReservationSend],
            data_segments: vec![DataSegment {
                offset: rid_pid_value_offset,
                value: rid_pid_values,
            }],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // rid_pid_value ptr
                    Counter(rid_pid_value_offset, 80),
                    // payload ptr
                    Regular(Instruction::I32Const(payload_offset as i32)),
                    // payload len
                    Regular(Instruction::I32Const(payload_len as i32)),
                    // delay
                    Regular(Instruction::I32Const(10)),
                    // err_mid ptr
                    Counter(err_mid_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        // insert gas reservation slots
        let program_id = ProgramId::from_origin(instance.addr);
        ProgramStorageOf::<T>::update_active_program(program_id, |program, _bn| {
            for x in 0..r * API_BENCHMARK_BATCH_SIZE {
                program.gas_reservation_map.insert(
                    ReservationId::from(x as u64),
                    GasReservationSlot {
                        amount: 1_000,
                        start: 1,
                        finish: 100,
                    },
                );
            }
        })
        .unwrap();

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(program_id),
            vec![],
            err_len_ptrs,
            Default::default(),
        )
    }

    // Benchmark the `gr_send_commit` call.
    // `gr_send` call is shortcut for `gr_send_init` + `gr_send_commit`
    pub fn gr_reservation_send_commit_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let rid_pid_value_offset = 1;

        let rid_pid_values = (0..API_BENCHMARK_BATCH_SIZE)
            .flat_map(|i| {
                let mut bytes = [0; 80];
                bytes[..32].copy_from_slice(ReservationId::from(i as u64).as_ref());
                bytes.to_vec()
            })
            .collect::<Vec<_>>();

        let payload_offset = rid_pid_value_offset + rid_pid_values.len() as u32;
        let payload_len = n * 1024;

        let err_mid_offset = payload_offset + payload_len;
        let err_len_ptrs = Self::err_len_ptrs(API_BENCHMARK_BATCH_SIZE, err_mid_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReservationSend],
            data_segments: vec![DataSegment {
                offset: rid_pid_value_offset,
                value: rid_pid_values,
            }],
            handle_body: Some(body::repeated_dyn(
                API_BENCHMARK_BATCH_SIZE,
                vec![
                    // rid_pid_value ptr
                    Counter(rid_pid_value_offset, 80),
                    // payload ptr
                    Regular(Instruction::I32Const(payload_offset as i32)),
                    // payload len
                    Regular(Instruction::I32Const(payload_len as i32)),
                    // delay
                    Regular(Instruction::I32Const(10)),
                    // err_mid ptr
                    Counter(err_mid_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        // insert gas reservation slots
        let program_id = ProgramId::from_origin(instance.addr);
        ProgramStorageOf::<T>::update_active_program(program_id, |program, _bn| {
            for x in 0..API_BENCHMARK_BATCH_SIZE {
                program.gas_reservation_map.insert(
                    ReservationId::from(x as u64),
                    GasReservationSlot {
                        amount: 1_000,
                        start: 1,
                        finish: 100,
                    },
                );
            }
        })
        .unwrap();

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(program_id),
            vec![],
            err_len_ptrs,
            Default::default(),
        )
    }

    pub fn gr_reply_commit(r: u32) -> Result<Exec<T>, &'static str> {
        let value_offset = 1;

        let err_mid_offset = value_offset + mem::size_of::<u128>() as u32;
        let err_len_ptrs = Self::err_len_ptrs(r, err_mid_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReplyCommit],
            handle_body: Some(body::repeated_dyn(
                r,
                vec![
                    // value ptr
                    Regular(Instruction::I32Const(value_offset as i32)),
                    // delay
                    Regular(Instruction::I32Const(10)),
                    // err_mid ptr
                    Counter(err_mid_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 10000000, err_len_ptrs)
    }

    pub fn gr_reply_push(r: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = 100;

        let err_offset = payload_offset + payload_len;
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, err_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReplyPush],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // payload ptr
                    Regular(Instruction::I32Const(payload_offset as i32)),
                    // payload len
                    Regular(Instruction::I32Const(payload_len as i32)),
                    // err len ptr
                    Counter(err_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 10000000, err_len_ptrs)
    }

    pub fn gr_reply_push_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = n as i32 * 1024;

        let err_offset = payload_offset + payload_len;
        let err_len_ptrs = Self::err_len_ptrs(1, err_offset as u32);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReplyPush],
            handle_body: Some(body::plain(vec![
                // payload ptr
                Instruction::I32Const(payload_offset),
                // payload len
                Instruction::I32Const(payload_len),
                // err len ptr
                Instruction::I32Const(err_offset),
                // CALL
                Instruction::Call(0),
                Instruction::End,
            ])),
            ..Default::default()
        });
        Self::prepare_handle(code, 10000000, err_len_ptrs)
    }

    pub fn gr_reservation_reply_commit(r: u32) -> Result<Exec<T>, &'static str> {
        let rid_value_offset = 1;

        let rid_values = (0..r)
            .flat_map(|i| {
                let mut bytes = [0; 32 + 16];
                bytes[..32].copy_from_slice(ReservationId::from(i as u64).as_ref());
                bytes.to_vec()
            })
            .collect::<Vec<_>>();

        let err_mid_offset = rid_value_offset + rid_values.len() as u32;
        let err_len_ptrs = Self::err_len_ptrs(r, err_mid_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReservationReplyCommit],
            data_segments: vec![DataSegment {
                offset: rid_value_offset,
                value: rid_values,
            }],
            handle_body: Some(body::repeated_dyn(
                r,
                vec![
                    // rid_value ptr
                    Counter(rid_value_offset, 48),
                    // delay
                    Regular(Instruction::I32Const(10)),
                    // err_mid ptr
                    Counter(err_mid_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        // insert gas reservation slots
        let program_id = ProgramId::from_origin(instance.addr);
        ProgramStorageOf::<T>::update_active_program(program_id, |program, _bn| {
            for x in 0..r * API_BENCHMARK_BATCH_SIZE {
                program.gas_reservation_map.insert(
                    ReservationId::from(x as u64),
                    GasReservationSlot {
                        amount: 1_000,
                        start: 1,
                        finish: 100,
                    },
                );
            }
        })
        .unwrap();

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(program_id),
            vec![],
            err_len_ptrs,
            Default::default(),
        )
    }

    pub fn gr_reply_to(r: u32) -> Result<Exec<T>, &'static str> {
        let err_mid_ptr = 1;
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, err_mid_ptr);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReplyTo],
            reply_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_mid ptr
                    Counter(err_mid_ptr, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let msg_id = MessageId::from(10);
        let msg = Message::new(
            msg_id,
            instance.addr.as_bytes().into(),
            ProgramId::from(instance.caller.clone().into_origin().as_bytes()),
            Default::default(),
            Some(1_000_000),
            0,
            None,
        )
        .into_stored();
        MailboxOf::<T>::insert(msg, u32::MAX.unique_saturated_into())
            .expect("Error during mailbox insertion");
        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Reply(msg_id, 0),
            vec![],
            err_len_ptrs,
            Default::default(),
        )
    }

    pub fn gr_signal_from(r: u32) -> Result<Exec<T>, &'static str> {
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::SignalFrom],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_mid ptr
                    Regular(Instruction::I32Const(0xffff)),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, 0..0)
    }

    pub fn gr_reply_push_input(r: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = 100;

        let err_offset = payload_offset + payload_len;
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, err_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReplyPushInput],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // offset
                    Regular(Instruction::I32Const(payload_offset as i32)),
                    Regular(Instruction::I32Const(
                        payload_len.saturating_sub(payload_offset) as i32,
                    )),
                    Counter(err_offset, Self::ERR_LEN_SIZE),
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;
        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![1u8; payload_len as usize],
            err_len_ptrs,
            Default::default(),
        )
    }

    pub fn gr_reply_push_input_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = n as i32 * 1_024;

        let err_offset = payload_offset + payload_len;
        let err_len_ptrs = Self::err_len_ptrs(1, err_offset as u32);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReplyPushInput],
            handle_body: Some(body::plain(vec![
                // offset
                Instruction::I32Const(payload_offset),
                Instruction::I32Const(payload_len.saturating_sub(payload_offset)),
                Instruction::I32Const(err_offset),
                Instruction::Call(0),
                Instruction::End,
            ])),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;
        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![1u8; payload_len as usize],
            err_len_ptrs,
            Default::default(),
        )
    }

    pub fn gr_send_push_input(r: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = 100u32;

        let err_handle_ptr = payload_offset + payload_len;

        let err_offset = err_handle_ptr + 8;
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, err_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::SendInit, SysCallName::SendPushInput],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_handle ptr for send_init
                    Regular(Instruction::I32Const(err_handle_ptr as i32)),
                    // CALL init
                    Regular(Instruction::Call(0)),
                    // handle
                    Regular(Instruction::I32Const((err_handle_ptr + 4) as i32)),
                    Regular(Instruction::I32Load(2, 0)),
                    // offset
                    Regular(Instruction::I32Const(payload_offset as i32)),
                    // len
                    Regular(Instruction::I32Const(
                        payload_len.saturating_sub(payload_offset) as i32,
                    )),
                    // err len ptr
                    Counter(err_offset, Self::ERR_LEN_SIZE),
                    // CALL push
                    Regular(Instruction::Call(1)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;
        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![1u8; payload_len as usize],
            err_len_ptrs,
            Default::default(),
        )
    }

    pub fn gr_send_push_input_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = n * 1_024;

        let err_handle_ptr = payload_offset + payload_len;

        let err_offset = err_handle_ptr + 8;
        let err_len_ptrs = Self::err_len_ptrs(API_BENCHMARK_BATCH_SIZE, err_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::SendInit, SysCallName::SendPushInput],
            handle_body: Some(body::repeated_dyn(
                API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_handle ptr for send_init
                    Regular(Instruction::I32Const(err_handle_ptr as i32)),
                    // CALL init
                    Regular(Instruction::Call(0)),
                    // handle
                    Regular(Instruction::I32Const((err_handle_ptr + 4) as i32)),
                    Regular(Instruction::I32Load(2, 0)),
                    // offset
                    Regular(Instruction::I32Const(payload_offset as i32)),
                    // len
                    Regular(Instruction::I32Const(
                        payload_len.saturating_sub(payload_offset) as i32,
                    )),
                    // err len ptr
                    Counter(err_offset, Self::ERR_LEN_SIZE),
                    // CALL push
                    Regular(Instruction::Call(1)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;
        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![1u8; payload_len as usize],
            err_len_ptrs,
            Default::default(),
        )
    }

    pub fn gr_status_code(r: u32) -> Result<Exec<T>, &'static str> {
        let err_code_ptr = 1;
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, err_code_ptr);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::StatusCode],
            reply_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_code ptr
                    Counter(err_code_ptr, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let msg_id = MessageId::from(10);
        let msg = Message::new(
            msg_id,
            instance.addr.as_bytes().into(),
            ProgramId::from(instance.caller.clone().into_origin().as_bytes()),
            Default::default(),
            Some(1_000_000),
            0,
            None,
        )
        .into_stored();
        MailboxOf::<T>::insert(msg, u32::MAX.unique_saturated_into())
            .expect("Error during mailbox insertion");
        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Reply(msg_id, 0),
            vec![],
            err_len_ptrs,
            Default::default(),
        )
    }

    pub fn gr_debug(r: u32) -> Result<Exec<T>, &'static str> {
        let string_offset = 1;
        let string_len = Self::GR_DEBUG_STRING_LEN;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Debug],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    // payload ptr
                    Instruction::I32Const(string_offset),
                    // payload len
                    Instruction::I32Const(string_len as i32),
                    // CALL
                    Instruction::Call(0),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, 0..0)
    }

    pub fn gr_debug_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let string_offset = 1;
        let string_len = n as i32 * 1024;
        assert!(
            WasmPage::from_offset((string_offset + string_len) as u32) < Self::MAX_PAGES.into()
        );

        // Access const amount of pages before debug call in order to remove lazy-pages factor.
        let instrs = body::read_access_all_pages_instrs(Self::MAX_PAGES.into(), vec![]);

        let instrs = body::repeated_dyn_instr(
            API_BENCHMARK_BATCH_SIZE,
            vec![
                // payload ptr
                Regular(Instruction::I32Const(string_offset)),
                // payload len
                Regular(Instruction::I32Const(string_len)),
                // CALL
                Regular(Instruction::Call(0)),
            ],
            instrs,
        );

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Debug],
            handle_body: Some(body::from_instructions(instrs)),
            ..Default::default()
        });

        Self::prepare_handle(code, 0, 0..0)
    }

    pub fn gr_error(r: u32) -> Result<Exec<T>, &'static str> {
        let status_code_offset = 1;
        let error_len_offset = status_code_offset + size_of::<u32>() as u32;
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, error_len_offset);
        let error_offset = err_len_ptrs.end;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::StatusCode, SysCallName::Error],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_code ptr
                    Regular(Instruction::I32Const(status_code_offset as i32)),
                    // CALL
                    Regular(Instruction::Call(0)),
                    // error data ptr of previous syscall
                    Regular(Instruction::I32Const(error_offset as i32)),
                    // error length ptr of `gr_error`
                    Counter(error_len_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(1)),
                ],
            )),
            ..Default::default()
        });

        Self::prepare_handle(code, 0, err_len_ptrs)
    }

    pub fn termination_bench(
        name: SysCallName,
        param: Option<u32>,
        r: u32,
    ) -> Result<Exec<T>, &'static str> {
        assert!(r <= 1);

        let instructions = if let Some(c) = param {
            assert!(name.signature().params.len() == 1);
            vec![Instruction::I32Const(c as i32), Instruction::Call(0)]
        } else {
            assert!(name.signature().params.is_empty());
            vec![Instruction::Call(0)]
        };

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![name],
            handle_body: Some(body::repeated(r, &instructions)),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, 0..0)
    }

    pub fn gr_wake(r: u32) -> Result<Exec<T>, &'static str> {
        let message_id_offset = 1;

        let message_ids = (0..r * API_BENCHMARK_BATCH_SIZE)
            .flat_map(|i| <[u8; 32]>::from(MessageId::from(i as u64)).to_vec())
            .collect::<Vec<_>>();

        let err_offset = message_id_offset + message_ids.len() as u32;
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, err_offset);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Wake],
            data_segments: vec![DataSegment {
                offset: message_id_offset,
                value: message_ids.to_vec(),
            }],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // message_id ptr
                    Counter(message_id_offset, 32),
                    // delay
                    Regular(Instruction::I32Const(10)),
                    // err len ptr
                    Counter(err_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            err_len_ptrs,
            Default::default(),
        )
    }

    pub fn gr_create_program_wgas(r: u32) -> Result<Exec<T>, &'static str> {
        let module = WasmModule::<T>::dummy();

        let cid_value_offset = 1;
        let mut cid_value = [0; 32 + 16];
        cid_value[0..32].copy_from_slice(module.hash.as_ref());
        cid_value[32..].copy_from_slice(&0u128.to_le_bytes());

        let payload_offset = cid_value_offset + cid_value.len() as u32;
        let payload = vec![0; 10];
        let payload_len = payload.len() as u32;

        let salt_len = 32;

        let err_mid_pid_offset = payload_offset + payload_len;
        let err_len_ptrs = Self::err_len_ptrs(r * API_BENCHMARK_BATCH_SIZE, err_mid_pid_offset);

        let _ = Gear::<T>::upload_code_raw(
            RawOrigin::Signed(benchmarking::account("instantiator", 0, 0)).into(),
            module.code,
        );

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::CreateProgramWGas],
            data_segments: vec![
                DataSegment {
                    offset: cid_value_offset,
                    value: cid_value.to_vec(),
                },
                DataSegment {
                    offset: payload_offset,
                    value: payload,
                },
            ],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // cid_value ptr
                    Regular(Instruction::I32Const(cid_value_offset as i32)),
                    // salt ptr
                    Counter(err_mid_pid_offset, Self::ERR_LEN_SIZE),
                    // salt len
                    Regular(Instruction::I32Const(salt_len)),
                    // payload ptr
                    Regular(Instruction::I32Const(payload_offset as i32)),
                    // payload len
                    Regular(Instruction::I32Const(payload_len as i32)),
                    // gas limit
                    Regular(Instruction::I64Const(100_000_000)),
                    // delay
                    Regular(Instruction::I32Const(10)),
                    // err_mid_pid ptr
                    Counter(err_mid_pid_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, err_len_ptrs)
    }

    pub fn gr_create_program_wgas_per_kb(pkb: u32, skb: u32) -> Result<Exec<T>, &'static str> {
        let module = WasmModule::<T>::dummy();

        let cid_value_offset = 1;
        let mut cid_value = [0; 32 + 16];
        cid_value[0..32].copy_from_slice(module.hash.as_ref());
        cid_value[32..].copy_from_slice(&0u128.to_le_bytes());

        let salt_offset = cid_value_offset + cid_value.len() as u32;
        let salt_len = skb * 1024;

        let payload_offset = salt_offset + salt_len;
        let payload_len = pkb * 1024;

        let err_mid_pid_offset = payload_offset + payload_len;
        let err_len_ptrs = Self::err_len_ptrs(API_BENCHMARK_BATCH_SIZE, err_mid_pid_offset);

        let _ = Gear::<T>::upload_code_raw(
            RawOrigin::Signed(benchmarking::account("instantiator", 0, 0)).into(),
            module.code,
        );
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::CreateProgramWGas],
            data_segments: vec![DataSegment {
                offset: cid_value_offset,
                value: cid_value.to_vec(),
            }],
            handle_body: Some(body::repeated_dyn(
                API_BENCHMARK_BATCH_SIZE,
                vec![
                    // cid_value ptr
                    Regular(Instruction::I32Const(cid_value_offset as i32)),
                    // salt ptr
                    Counter(err_mid_pid_offset, Self::ERR_LEN_SIZE),
                    // salt len
                    Regular(Instruction::I32Const(salt_len as i32)),
                    // payload ptr
                    Regular(Instruction::I32Const(payload_offset as i32)),
                    // payload len
                    Regular(Instruction::I32Const(payload_len as i32)),
                    // gas limit
                    Regular(Instruction::I64Const(100_000_000)),
                    // delay
                    Regular(Instruction::I32Const(10)),
                    // err_mid_pid ptr
                    Counter(err_mid_pid_offset, Self::ERR_LEN_SIZE),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        Self::prepare_handle(code, 0, err_len_ptrs)
    }

    pub fn lazy_pages_read(wasm_pages: WasmPage) -> Result<Exec<T>, &'static str> {
        let instrs = body::read_access_all_pages_instrs(wasm_pages, vec![]);
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, 0..0)
    }

    pub fn lazy_pages_write(wasm_pages: WasmPage) -> Result<Exec<T>, &'static str> {
        let instrs = body::write_access_all_pages_instrs(wasm_pages, vec![]);
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, 0..0)
    }

    pub fn lazy_pages_write_after_read(wasm_pages: WasmPage) -> Result<Exec<T>, &'static str> {
        let mut instrs = body::read_access_all_pages_instrs(max_pages::<T>().into(), vec![]);
        instrs = body::write_access_all_pages_instrs(wasm_pages, instrs);
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, 0..0)
    }

    pub fn lazy_pages_read_storage_data(wasm_pages: WasmPage) -> Result<Exec<T>, &'static str> {
        let exec = Self::lazy_pages_read(wasm_pages)?;
        let program_id = exec.context.program().id();
        for page in wasm_pages
            .iter_from_zero()
            .flat_map(|p| p.to_pages_iter::<GearPage>())
        {
            ProgramStorageOf::<T>::set_program_page_data(
                program_id,
                page,
                PageBuf::from_inner(PageBufInner::filled_with(1)),
            );
        }
        Ok(exec)
    }

    pub fn host_func_read(wasm_pages: WasmPage) -> Result<Exec<T>, &'static str> {
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Debug],
            handle_body: Some(body::from_instructions(vec![
                // payload ptr
                Instruction::I32Const(0),
                // payload len
                Instruction::I32Const(wasm_pages.offset() as i32),
                // CALL
                Instruction::Call(0),
            ])),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, 0..0)
    }

    pub fn host_func_write(wasm_pages: WasmPage) -> Result<Exec<T>, &'static str> {
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Read],
            handle_body: Some(body::from_instructions(vec![
                // at
                Instruction::I32Const(0),
                // len
                Instruction::I32Const(wasm_pages.offset() as i32),
                // buffer ptr
                Instruction::I32Const(0),
                // err len ptr
                Instruction::I32Const(0),
                // CALL
                Instruction::Call(0),
            ])),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![0xff; MAX_PAYLOAD_LEN as usize],
            0..0,
            Default::default(),
        )
    }

    pub fn host_func_write_after_read(wasm_pages: WasmPage) -> Result<Exec<T>, &'static str> {
        assert!(wasm_pages <= Self::MAX_PAGES.into());

        // Access const amount of pages before `gr_read` calls in order to make all pages read accessed.
        let mut instrs = body::read_access_all_pages_instrs(Self::MAX_PAGES.into(), vec![]);
        instrs.extend_from_slice(&[
            // at
            Instruction::I32Const(0),
            // len
            Instruction::I32Const(wasm_pages.offset() as i32),
            // buffer ptr
            Instruction::I32Const(0),
            // err len ptr
            Instruction::I32Const(0),
            // CALL
            Instruction::Call(0),
        ]);

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Read],
            handle_body: Some(body::from_instructions(instrs)),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![0xff; MAX_PAYLOAD_LEN as usize],
            0..0,
            Default::default(),
        )
    }
}
