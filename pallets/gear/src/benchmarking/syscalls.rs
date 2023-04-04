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
    Exec, Program, API_BENCHMARK_BATCHES,
};
use crate::{
    benchmarking::MAX_PAYLOAD_LEN, manager::HandleKind, schedule::API_BENCHMARK_BATCH_SIZE, Config,
    MailboxOf, Pallet as Gear, ProgramStorageOf,
};
use alloc::{vec, vec::Vec};
use common::{benchmarking, storage::*, Origin, ProgramStorage};
use core::{marker::PhantomData, mem::size_of};
use frame_system::RawOrigin;
use gear_core::{
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::{GearPage, PageBuf, PageBufInner, PageU32Size, WasmPage},
    message::{Message, Value},
    reservation::GasReservationSlot,
};
use gear_wasm_instrument::{parity_wasm::elements::Instruction, syscalls::SysCallName};
use sp_core::Get;
use sp_runtime::{codec::Encode, traits::UniqueSaturatedInto};

/// Size of fallible syscall error length
const ERR_LEN_SIZE: u32 = size_of::<u32>() as u32;
/// Handle size
const HANDLE_SIZE: u32 = size_of::<u32>() as u32;
/// Value size
const VALUE_SIZE: u32 = size_of::<Value>() as u32;
/// Reservation id size
const RID_SIZE: u32 = size_of::<ReservationId>() as u32;
/// Code id size
const CID_SIZE: u32 = size_of::<CodeId>() as u32;
/// Program id size
const PID_SIZE: u32 = size_of::<ProgramId>() as u32;
/// Message id size
const MID_SIZE: u32 = size_of::<MessageId>() as u32;
/// Random subject size
const RANDOM_SUBJECT_SIZE: u32 = 32;

/// Size of struct with fields: error len and handle
const ERR_HANDLE_SIZE: u32 = ERR_LEN_SIZE + HANDLE_SIZE;
/// Size of struct with fields: reservation id and value
const RID_VALUE_SIZE: u32 = RID_SIZE + VALUE_SIZE;
/// Size of struct with fields: program id and value
const PID_VALUE_SIZE: u32 = PID_SIZE + VALUE_SIZE;
/// Size of struct with fields: code id and value
const CID_VALUE_SIZE: u32 = CID_SIZE + VALUE_SIZE;
/// Size of struct with fields: reservation id, program id and value
const RID_PID_VALUE_SIZE: u32 = RID_SIZE + PID_SIZE + VALUE_SIZE;

/// Size of memory with one wasm page
const SMALL_MEM_SIZE: u16 = 1;
/// Common offset for data in memory. We use `1` to make memory accesses unaligned
/// and therefore slower, because we wanna to identify max weights.
const COMMON_OFFSET: u32 = 1;
/// Common small payload len.
const COMMON_PAYLOAD_LEN: u32 = 100;

const MAX_REPETITIONS: u32 = API_BENCHMARK_BATCHES * API_BENCHMARK_BATCH_SIZE;

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
    fn prepare_handle(module: ModuleDefinition, value: u32) -> Result<Exec<T>, &'static str> {
        let instance = Program::<T>::new(module.into(), vec![])?;
        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            PrepareConfig {
                value: value.into(),
                ..Default::default()
            },
        )
    }

    fn prepare_handle_with_reservation_slots(
        module: ModuleDefinition,
        repetitions: u32,
    ) -> Result<Exec<T>, &'static str> {
        let instance = Program::<T>::new(module.into(), vec![])?;

        // insert gas reservation slots
        let program_id = ProgramId::from_origin(instance.addr);
        ProgramStorageOf::<T>::update_active_program(program_id, |program, _bn| {
            for x in 0..repetitions {
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
            Default::default(),
        )
    }

    fn prepare_handle_with_const_payload(
        module: ModuleDefinition,
    ) -> Result<Exec<T>, &'static str> {
        let instance = Program::<T>::new(module.into(), vec![])?;
        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![0xff; MAX_PAYLOAD_LEN as usize],
            Default::default(),
        )
    }

    // TODO: add check for alloc result #2498
    pub fn alloc(r: u32) -> Result<Exec<T>, &'static str> {
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(0)),
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
        };

        Self::prepare_handle(module, 0)
    }

    // TODO: add check for alloc and free result #2498
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

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(0)),
            imported_functions: vec![SysCallName::Alloc, SysCallName::Free],
            init_body: None,
            handle_body: Some(body::from_instructions(instructions)),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_reserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r;
        let res_offset = COMMON_OFFSET;

        // It is not allowed to reserve less than mailbox threshold
        let mailbox_threshold = <T as Config>::MailboxThreshold::get();

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::ReserveGas],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // gas amount
                    InstrI64Const(mailbox_threshold),
                    // duration
                    InstrI32Const(1),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_unreserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r;
        assert!(repetitions <= MAX_REPETITIONS);

        // Store max repetitions for any `r` to exclude data segments size contribution.
        let reservation_id_bytes: Vec<u8> = (0..MAX_REPETITIONS)
            .map(|i| ReservationId::from(i as u64))
            .flat_map(|x| x.encode())
            .collect();

        let reservation_id_offset = COMMON_OFFSET;
        let res_offset = reservation_id_offset + reservation_id_bytes.len() as u32;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::UnreserveGas],
            data_segments: vec![DataSegment {
                offset: reservation_id_offset,
                value: reservation_id_bytes,
            }],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // reservation id offset
                    Counter(reservation_id_offset, RID_SIZE),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle_with_reservation_slots(module, repetitions)
    }

    pub fn gr_system_reserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let res_offset = COMMON_OFFSET;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::SystemReserveGas],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // gas amount
                    InstrI64Const(50_000_000),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn getter(name: SysCallName, r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let res_offset = COMMON_OFFSET;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![name],
            handle_body: Some(body::syscall(
                repetitions,
                &[
                    // offset where to write taken data
                    InstrI32Const(res_offset),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_read(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let buffer_offset = COMMON_OFFSET;
        let buffer_len = COMMON_PAYLOAD_LEN;
        let res_offset = buffer_offset + buffer_len;

        assert!(buffer_len <= MAX_PAYLOAD_LEN);

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::Read],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // at
                    InstrI32Const(0),
                    // len
                    InstrI32Const(buffer_len),
                    // buffer offset
                    InstrI32Const(buffer_offset),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle_with_const_payload(module)
    }

    pub fn gr_read_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = API_BENCHMARK_BATCH_SIZE;
        let buffer_offset = COMMON_OFFSET;
        let buffer_len = n * 1024;
        let res_offset = buffer_offset + buffer_len;

        assert!(buffer_len <= MAX_PAYLOAD_LEN);

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Read],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // at
                    InstrI32Const(0),
                    // len
                    InstrI32Const(buffer_len),
                    // buffer offset
                    InstrI32Const(buffer_offset),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle_with_const_payload(module)
    }

    pub fn gr_random(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let subject_offset = COMMON_OFFSET;
        let bn_random_offset = subject_offset + RANDOM_SUBJECT_SIZE;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::Random],
            handle_body: Some(body::syscall(
                repetitions,
                &[
                    // subject offset
                    InstrI32Const(subject_offset),
                    // bn random offset
                    InstrI32Const(bn_random_offset),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_send_init(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let res_offset = COMMON_OFFSET;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::SendInit],
            handle_body: Some(body::fallible_syscall(repetitions, res_offset, &[])),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_send_push(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        assert!(repetitions <= MAX_REPETITIONS);

        let payload_offset = COMMON_OFFSET;
        let payload_len = COMMON_PAYLOAD_LEN;
        let res_offset = payload_offset + payload_len;
        let err_handle_offset = res_offset + ERR_LEN_SIZE;

        let mut instructions = body::fallible_syscall_instr(
            MAX_REPETITIONS,
            1,
            Counter(err_handle_offset, ERR_HANDLE_SIZE),
            &[],
        );
        instructions.extend(body::fallible_syscall_instr(
            repetitions,
            0,
            InstrI32Const(res_offset),
            &[
                // get handle from send init results
                Counter(err_handle_offset + ERR_LEN_SIZE, ERR_HANDLE_SIZE),
                InstrI32Load(2, 0),
                // payload ptr
                InstrI32Const(payload_offset),
                // payload len
                InstrI32Const(payload_len),
            ],
        ));

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::SendPush, SysCallName::SendInit],
            handle_body: Some(body::from_instructions(instructions)),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_send_push_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = API_BENCHMARK_BATCH_SIZE;
        let payload_offset = COMMON_OFFSET;
        let payload_len = n * 1024;
        let res_offset = payload_offset + payload_len;
        let err_handle_offset = res_offset + ERR_LEN_SIZE;

        let mut instructions = body::fallible_syscall_instr(
            API_BENCHMARK_BATCH_SIZE,
            1,
            Counter(err_handle_offset, ERR_HANDLE_SIZE),
            &[],
        );
        instructions.extend(body::fallible_syscall_instr(
            repetitions,
            0,
            InstrI32Const(res_offset),
            &[
                // get handle from send init results
                Counter(err_handle_offset + ERR_LEN_SIZE, ERR_HANDLE_SIZE),
                InstrI32Load(2, 0),
                // payload ptr
                InstrI32Const(payload_offset),
                // payload len
                InstrI32Const(payload_len),
            ],
        ));

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::SendPush, SysCallName::SendInit],
            handle_body: Some(body::from_instructions(instructions)),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    // Benchmark the `gr_send_commit` call.
    // `gr_send` call is shortcut for `gr_send_init` + `gr_send_commit`
    pub fn gr_send_commit(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let pid_value_offset = COMMON_OFFSET;
        let payload_offset = pid_value_offset + PID_VALUE_SIZE;
        let payload_len = COMMON_PAYLOAD_LEN;
        let res_offset = payload_offset + payload_len;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::Send],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // pid value offset
                    InstrI32Const(pid_value_offset),
                    // payload offset
                    InstrI32Const(payload_offset),
                    // payload len
                    InstrI32Const(payload_len),
                    // delay
                    InstrI32Const(10),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 10000000)
    }

    // Benchmark the `gr_send_commit` call.
    // `gr_send` call is shortcut for `gr_send_init` + `gr_send_commit`
    pub fn gr_send_commit_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = API_BENCHMARK_BATCH_SIZE;
        let pid_value_offset = COMMON_OFFSET;
        let payload_offset = pid_value_offset + PID_VALUE_SIZE;
        let payload_len = n * 1024;
        let res_offset = payload_offset + payload_len;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Send],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // pid value offset
                    InstrI32Const(pid_value_offset),
                    // payload offset
                    InstrI32Const(payload_offset),
                    // payload len
                    InstrI32Const(payload_len),
                    // delay
                    InstrI32Const(10),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 10000000)
    }

    // Benchmark the `gr_reservation_send_commit` call.
    // `gr_send` call is shortcut for `gr_send_init` + `gr_send_commit`
    pub fn gr_reservation_send_commit(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        assert!(repetitions <= MAX_REPETITIONS);

        let rid_pid_values: Vec<u8> = (0..MAX_REPETITIONS)
            .flat_map(|i| {
                let mut bytes = [0; RID_PID_VALUE_SIZE as usize];
                bytes[..RID_SIZE as usize].copy_from_slice(ReservationId::from(i as u64).as_ref());
                bytes
            })
            .collect();

        let rid_pid_value_offset = COMMON_OFFSET;
        let payload_offset = rid_pid_value_offset + rid_pid_values.len() as u32;
        let payload_len = COMMON_PAYLOAD_LEN;
        let res_offset = payload_offset + payload_len;

        let module = ModuleDefinition {
            // One `SMALL_MEM_SIZE + 1` in order to fit data segments in memory
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE + 1)),
            imported_functions: vec![SysCallName::ReservationSend],
            data_segments: vec![DataSegment {
                offset: rid_pid_value_offset,
                value: rid_pid_values,
            }],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // rid pid value offset
                    Counter(rid_pid_value_offset, RID_PID_VALUE_SIZE),
                    // payload offset
                    InstrI32Const(payload_offset),
                    // payload len
                    InstrI32Const(payload_len),
                    // delay
                    InstrI32Const(10),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle_with_reservation_slots(module, repetitions)
    }

    // Benchmark the `gr_send_commit` call.
    // `gr_send` call is shortcut for `gr_send_init` + `gr_send_commit`
    pub fn gr_reservation_send_commit_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = API_BENCHMARK_BATCH_SIZE;

        let rid_pid_values = (0..repetitions)
            .flat_map(|i| {
                let mut bytes = [0; RID_PID_VALUE_SIZE as usize];
                bytes[..RID_SIZE as usize].copy_from_slice(ReservationId::from(i as u64).as_ref());
                bytes
            })
            .collect::<Vec<_>>();

        let rid_pid_value_offset = COMMON_OFFSET;
        let payload_offset = rid_pid_value_offset + rid_pid_values.len() as u32;
        let payload_len = n * 1024;
        let res_offset = payload_offset + payload_len;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReservationSend],
            data_segments: vec![DataSegment {
                offset: rid_pid_value_offset,
                value: rid_pid_values,
            }],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // rid pid value offset
                    Counter(rid_pid_value_offset, RID_PID_VALUE_SIZE),
                    // payload offset
                    InstrI32Const(payload_offset),
                    // payload len
                    InstrI32Const(payload_len),
                    // delay
                    InstrI32Const(10),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle_with_reservation_slots(module, repetitions)
    }

    pub fn gr_reply_commit(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r;
        assert!(repetitions <= 1);

        let value_offset = COMMON_OFFSET;
        let res_offset = value_offset + VALUE_SIZE;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::ReplyCommit],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // value offset
                    InstrI32Const(value_offset),
                    // delay
                    InstrI32Const(10),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 10000000)
    }

    pub fn gr_reply_commit_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = 1;
        let payload_offset = COMMON_OFFSET;
        let payload_len = n * 1024;
        let res_offset = payload_offset + payload_len;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Reply],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // payload ptr
                    InstrI32Const(payload_offset),
                    // payload len
                    InstrI32Const(payload_len),
                    // value ptr
                    InstrI32Const(payload_offset),
                    // delay
                    InstrI32Const(10),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 10000000)
    }

    pub fn gr_reply_push(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let payload_offset = COMMON_OFFSET;
        let payload_len = COMMON_PAYLOAD_LEN;
        let res_offset = payload_offset + payload_len;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::ReplyPush],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // payload ptr
                    InstrI32Const(payload_offset),
                    // payload len
                    InstrI32Const(payload_len),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 10000000)
    }

    pub fn gr_reply_push_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = 1;
        let payload_offset = COMMON_OFFSET;
        let payload_len = n * 1024;
        let res_offset = payload_offset + payload_len;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReplyPush],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // payload ptr
                    InstrI32Const(payload_offset),
                    // payload len
                    InstrI32Const(payload_len),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 10000000)
    }

    pub fn gr_reservation_reply_commit(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r;
        let max_repetitions = 1;
        assert!(repetitions <= max_repetitions);

        let rid_values: Vec<_> = (0..max_repetitions)
            .flat_map(|i| {
                let mut bytes = [0; RID_VALUE_SIZE as usize];
                bytes[..RID_SIZE as usize].copy_from_slice(ReservationId::from(i as u64).as_ref());
                bytes.to_vec()
            })
            .collect();

        let rid_value_offset = COMMON_OFFSET;
        let res_offset = rid_value_offset + rid_values.len() as u32;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::ReservationReplyCommit],
            data_segments: vec![DataSegment {
                offset: rid_value_offset,
                value: rid_values,
            }],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // rid_value ptr
                    Counter(rid_value_offset, RID_VALUE_SIZE),
                    // delay
                    InstrI32Const(10),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle_with_reservation_slots(module, repetitions)
    }

    pub fn gr_reservation_reply_commit_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = 1;
        let rid_value_offset = COMMON_OFFSET;
        let payload_offset = rid_value_offset + RID_VALUE_SIZE;
        let payload_len = n * 1024;
        let res_offset = payload_offset + payload_len;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReservationReply],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // rid_value ptr
                    InstrI32Const(rid_value_offset),
                    // payload ptr
                    InstrI32Const(payload_offset),
                    // payload len
                    InstrI32Const(payload_len),
                    // delay
                    InstrI32Const(10),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle_with_reservation_slots(module, repetitions)
    }

    pub fn gr_reply_to(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let res_offset = COMMON_OFFSET;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::ReplyTo],
            reply_body: Some(body::fallible_syscall(repetitions, res_offset, &[])),
            ..Default::default()
        };

        let instance = Program::<T>::new(module.into(), vec![])?;

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
            Default::default(),
        )
    }

    pub fn gr_signal_from(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let res_offset = COMMON_OFFSET;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::SignalFrom],
            handle_body: Some(body::syscall(repetitions, &[InstrI32Const(res_offset)])),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_reply_push_input(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let input_at = 0;
        let input_len = COMMON_PAYLOAD_LEN;
        let res_offset = COMMON_OFFSET;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::ReplyPushInput],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // input at
                    InstrI32Const(input_at),
                    // input len
                    InstrI32Const(input_len),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle_with_const_payload(module)
    }

    pub fn gr_reply_push_input_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = 1;
        let input_at = 0;
        let input_len = n * 1024;
        let res_offset = COMMON_OFFSET;

        assert!(input_len <= MAX_PAYLOAD_LEN);

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReplyPushInput],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // input at
                    InstrI32Const(input_at),
                    // input len
                    InstrI32Const(input_len),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle_with_const_payload(module)
    }

    pub fn gr_send_push_input(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        assert!(repetitions <= MAX_REPETITIONS);

        let input_at = 0;
        let input_len = COMMON_PAYLOAD_LEN;
        let res_offset = COMMON_OFFSET;
        let err_handle_offset = COMMON_OFFSET + ERR_LEN_SIZE;

        let mut instructions = body::fallible_syscall_instr(
            MAX_REPETITIONS,
            1,
            Counter(err_handle_offset, ERR_HANDLE_SIZE),
            &[],
        );

        instructions.extend(
            body::fallible_syscall_instr(
                repetitions,
                0,
                InstrI32Const(res_offset),
                &[
                    // get handle from send init results
                    Counter(err_handle_offset + ERR_LEN_SIZE, ERR_HANDLE_SIZE),
                    InstrI32Load(2, 0),
                    // input at
                    InstrI32Const(input_at),
                    // input len
                    InstrI32Const(input_len),
                ],
            )
            .into_iter(),
        );

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::SendPushInput, SysCallName::SendInit],
            handle_body: Some(body::from_instructions(instructions)),
            ..Default::default()
        };

        Self::prepare_handle_with_const_payload(module)
    }

    pub fn gr_send_push_input_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = API_BENCHMARK_BATCH_SIZE;
        let input_at = 0;
        let input_len = n * 1024;
        let res_offset = COMMON_OFFSET;
        let err_handle_offset = res_offset + ERR_LEN_SIZE;

        let mut instructions = body::fallible_syscall_instr(
            API_BENCHMARK_BATCH_SIZE,
            1,
            Counter(err_handle_offset, ERR_HANDLE_SIZE),
            &[],
        );

        instructions.extend(
            body::fallible_syscall_instr(
                repetitions,
                0,
                InstrI32Const(res_offset),
                &[
                    // get handle from send init results
                    Counter(err_handle_offset + ERR_LEN_SIZE, ERR_HANDLE_SIZE),
                    InstrI32Load(2, 0),
                    // input at
                    InstrI32Const(input_at),
                    // input len
                    InstrI32Const(input_len),
                ],
            )
            .into_iter(),
        );

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::SendPushInput, SysCallName::SendInit],
            handle_body: Some(body::from_instructions(instructions)),
            ..Default::default()
        };

        Self::prepare_handle_with_const_payload(module)
    }

    pub fn gr_status_code(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let res_offset = COMMON_OFFSET;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::StatusCode],
            reply_body: Some(body::fallible_syscall(repetitions, res_offset, &[])),
            ..Default::default()
        };

        let instance = Program::<T>::new(module.into(), vec![])?;

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
            Default::default(),
        )
    }

    pub fn gr_debug(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let string_offset = COMMON_OFFSET;
        let string_len = COMMON_PAYLOAD_LEN;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::Debug],
            handle_body: Some(body::syscall(
                repetitions,
                &[
                    // payload ptr
                    InstrI32Const(string_offset),
                    // payload len
                    InstrI32Const(string_len),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_debug_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = API_BENCHMARK_BATCH_SIZE;
        let string_offset = COMMON_OFFSET;
        let string_len = n * 1024;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Debug],
            handle_body: Some(body::syscall(
                repetitions,
                &[
                    // payload ptr
                    InstrI32Const(string_offset),
                    // payload len
                    InstrI32Const(string_len),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_error(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let res_offset = COMMON_OFFSET;
        let err_data_buffer_offset = res_offset + ERR_LEN_SIZE;

        let mut handle_body = body::fallible_syscall(
            repetitions,
            res_offset,
            &[
                // error encoded data buffer offset
                InstrI32Const(err_data_buffer_offset),
            ],
        );

        // Insert first `gr_error` call, which returns error, so all other `gr_error` calls will be Ok.
        handle_body.code_mut().elements_mut().splice(
            0..0,
            [
                Instruction::I32Const(0),
                Instruction::I32Const(0),
                Instruction::Call(0),
            ],
        );

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::Error],
            handle_body: Some(handle_body),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn termination_bench(
        name: SysCallName,
        param: Option<u32>,
        r: u32,
    ) -> Result<Exec<T>, &'static str> {
        let repetitions = r;
        assert!(repetitions <= 1);

        let params = if let Some(c) = param {
            assert!(name.signature().params.len() == 1);
            vec![InstrI32Const(c)]
        } else {
            assert!(name.signature().params.is_empty());
            vec![]
        };

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![name],
            handle_body: Some(body::syscall(repetitions, &params)),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_wake(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        assert!(repetitions <= MAX_REPETITIONS);

        let message_ids: Vec<u8> = (0..MAX_REPETITIONS)
            .flat_map(|i| <[u8; MID_SIZE as usize]>::from(MessageId::from(i as u64)).to_vec())
            .collect();

        let message_id_offset = COMMON_OFFSET;
        let res_offset = message_id_offset + message_ids.len() as u32;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::Wake],
            data_segments: vec![DataSegment {
                offset: message_id_offset,
                value: message_ids,
            }],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // message id offset
                    Counter(message_id_offset, MID_SIZE),
                    // delay
                    InstrI32Const(10),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_create_program_wgas(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;

        let module = WasmModule::<T>::dummy();
        let _ = Gear::<T>::upload_code_raw(
            RawOrigin::Signed(benchmarking::account("instantiator", 0, 0)).into(),
            module.code,
        );

        let mut cid_value = [0; CID_VALUE_SIZE as usize];
        cid_value[0..CID_SIZE as usize].copy_from_slice(module.hash.as_ref());
        cid_value[CID_SIZE as usize..].copy_from_slice(&0u128.to_le_bytes());

        let cid_value_offset = COMMON_OFFSET;
        let payload_offset = cid_value_offset + cid_value.len() as u32;
        let payload_len = 10;
        let res_offset = payload_offset + payload_len;

        // Use previous result bytes as salt. First one uses 0 bytes.
        let salt_offset = res_offset;
        let salt_len = 32;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SysCallName::CreateProgramWGas],
            data_segments: vec![DataSegment {
                offset: cid_value_offset,
                value: cid_value.to_vec(),
            }],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // cid value offset
                    InstrI32Const(cid_value_offset),
                    // salt offset
                    InstrI32Const(salt_offset),
                    // salt len
                    InstrI32Const(salt_len),
                    // payload offset
                    InstrI32Const(payload_offset),
                    // payload len
                    InstrI32Const(payload_len),
                    // gas limit
                    InstrI64Const(100_000_000),
                    // delay
                    InstrI32Const(10),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_create_program_wgas_per_kb(pkb: u32, skb: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = API_BENCHMARK_BATCH_SIZE;

        let module = WasmModule::<T>::dummy();
        let _ = Gear::<T>::upload_code_raw(
            RawOrigin::Signed(benchmarking::account("instantiator", 0, 0)).into(),
            module.code,
        );

        let mut cid_value = [0; (CID_SIZE + VALUE_SIZE) as usize];
        cid_value[0..CID_SIZE as usize].copy_from_slice(module.hash.as_ref());
        cid_value[CID_SIZE as usize..].copy_from_slice(&0u128.to_le_bytes());

        let cid_value_offset = COMMON_OFFSET;
        let payload_offset = cid_value_offset + cid_value.len() as u32;
        let payload_len = pkb * 1024;
        let res_offset = payload_offset + payload_len;

        // Use previous result bytes as part of salt buffer. First one uses 0 bytes.
        let salt_offset = res_offset;
        let salt_len = skb * 1024;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::CreateProgramWGas],
            data_segments: vec![DataSegment {
                offset: cid_value_offset,
                value: cid_value.to_vec(),
            }],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // cid_value offset
                    InstrI32Const(cid_value_offset),
                    // salt offset
                    InstrI32Const(salt_offset),
                    // salt len
                    InstrI32Const(salt_len),
                    // payload offset
                    InstrI32Const(payload_offset),
                    // payload len
                    InstrI32Const(payload_len),
                    // gas limit
                    InstrI64Const(100_000_000),
                    // delay
                    InstrI32Const(10),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn lazy_pages_signal_read(wasm_pages: WasmPage) -> Result<Exec<T>, &'static str> {
        let instrs = body::read_access_all_pages_instrs(wasm_pages, vec![]);
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            stack_end: Some(0.into()),
            ..Default::default()
        };
        Self::prepare_handle(module, 0)
    }

    pub fn lazy_pages_signal_write(wasm_pages: WasmPage) -> Result<Exec<T>, &'static str> {
        let instrs = body::write_access_all_pages_instrs(wasm_pages, vec![]);
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            stack_end: Some(0.into()),
            ..Default::default()
        };
        Self::prepare_handle(module, 0)
    }

    pub fn lazy_pages_signal_write_after_read(
        wasm_pages: WasmPage,
    ) -> Result<Exec<T>, &'static str> {
        let instrs = body::read_access_all_pages_instrs(max_pages::<T>().into(), vec![]);
        let instrs = body::write_access_all_pages_instrs(wasm_pages, instrs);
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            stack_end: Some(0.into()),
            ..Default::default()
        };
        Self::prepare_handle(module, 0)
    }

    pub fn lazy_pages_load_page_storage_data(
        wasm_pages: WasmPage,
    ) -> Result<Exec<T>, &'static str> {
        let exec = Self::lazy_pages_signal_read(wasm_pages)?;
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

    pub fn lazy_pages_host_func_read(wasm_pages: WasmPage) -> Result<Exec<T>, &'static str> {
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Debug],
            handle_body: Some(body::from_instructions(vec![
                // payload offset
                Instruction::I32Const(0),
                // payload len
                Instruction::I32Const(wasm_pages.offset() as i32),
                // CALL
                Instruction::Call(0),
            ])),
            stack_end: Some(0.into()),
            ..Default::default()
        };
        Self::prepare_handle(module, 0)
    }

    pub fn lazy_pages_host_func_write(wasm_pages: WasmPage) -> Result<Exec<T>, &'static str> {
        let module = ModuleDefinition {
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
            stack_end: Some(0.into()),
            ..Default::default()
        };

        Self::prepare_handle_with_const_payload(module)
    }

    pub fn lazy_pages_host_func_write_after_read(
        wasm_pages: WasmPage,
    ) -> Result<Exec<T>, &'static str> {
        let max_pages = WasmPage::from_offset(MAX_PAYLOAD_LEN);
        assert!(wasm_pages <= max_pages);

        // Access const amount of pages before `gr_read` calls in order to make all pages read accessed.
        let mut instrs = body::read_access_all_pages_instrs(max_pages, vec![]);

        // Add `gr_read` call.
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

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Read],
            handle_body: Some(body::from_instructions(instrs)),
            stack_end: Some(0.into()),
            ..Default::default()
        };

        Self::prepare_handle_with_const_payload(module)
    }
}
