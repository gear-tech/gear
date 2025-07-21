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

//! Benchmarks for gear syscalls.

use super::{
    API_BENCHMARK_BATCHES, Exec, Program,
    code::{
        DataSegment, ImportedMemory, ModuleDefinition, WasmModule,
        body::{self, DynInstr::*, unreachable_condition_i32},
    },
    utils::{self, PrepareConfig},
};
use crate::{
    Config, MailboxOf, Pallet as Gear, ProgramStorageOf, benchmarking::MAX_PAYLOAD_LEN,
    manager::HandleKind, schedule::API_BENCHMARK_BATCH_SIZE,
};
use alloc::{vec, vec::Vec};
use common::{Origin, ProgramStorage, storage::*};
use core::marker::PhantomData;
use gear_core::{
    ids::{ActorId, CodeId, MessageId, ReservationId},
    memory::{PageBuf, PageBufInner},
    message::{Message, Value},
    pages::{GearPage, WasmPage, WasmPagesAmount, numerated::iterators::IntervalIterator},
    reservation::GasReservationSlot,
};
use gear_core_errors::*;
use gear_wasm_instrument::{Instruction, SyscallName};
use rand::{SeedableRng, seq::SliceRandom};
use rand_pcg::Pcg64;
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
const PID_SIZE: u32 = size_of::<ActorId>() as u32;
/// Message id size
const MID_SIZE: u32 = size_of::<MessageId>() as u32;
/// Random subject size
const RANDOM_SUBJECT_SIZE: u32 = 32;

/// Size of struct with fields: error len and handle
const ERR_HANDLE_SIZE: u32 = ERR_LEN_SIZE + HANDLE_SIZE;
/// Size of struct with fields: error len and message id
const ERR_MID_SIZE: u32 = ERR_LEN_SIZE + MID_SIZE;
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

fn kb_to_bytes(size_in_kb: u32) -> u32 {
    size_in_kb.checked_mul(1024).unwrap()
}

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
            HandleKind::Handle(instance.addr.cast()),
            vec![],
            PrepareConfig {
                value: value.into(),
                ..Default::default()
            },
        )
    }

    fn prepare_signal_handle(
        module: ModuleDefinition,
        value: u32,
    ) -> Result<Exec<T>, &'static str> {
        let instance = Program::<T>::new(module.into(), vec![])?;

        // inserting a message with a signal which will be later handled by utils::prepare_exec
        let msg_id = MessageId::from(10);
        let signal_code = SignalCode::RemovedFromWaitlist;
        let msg = Message::new(
            msg_id,
            instance.addr.as_bytes().try_into().unwrap(),
            ActorId::try_from(instance.caller.clone().into_origin().as_bytes()).unwrap(),
            Default::default(),
            Some(1_000_000),
            0,
            None,
        )
        .into_stored();
        let msg = msg.try_into().expect("Error during message conversion");

        MailboxOf::<T>::insert(msg, u32::MAX.unique_saturated_into())
            .expect("Error during mailbox insertion");

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Signal(msg_id, signal_code),
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
        let program_id = instance.addr.cast();
        ProgramStorageOf::<T>::update_active_program(program_id, |program| {
            for x in 0..repetitions {
                program.gas_reservation_map.insert(
                    ReservationId::from(x as u64),
                    GasReservationSlot {
                        amount: 100_000,
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
            HandleKind::Handle(instance.addr.cast()),
            vec![0xff; MAX_PAYLOAD_LEN as usize],
            Default::default(),
        )
    }

    fn prepare_handle_with_allocations(
        module: ModuleDefinition,
        allocations: impl Iterator<Item = WasmPage>,
    ) -> Result<Exec<T>, &'static str> {
        let instance = Program::<T>::new(module.into(), vec![])?;

        let program_id = ActorId::from_origin(instance.addr);
        ProgramStorageOf::<T>::set_allocations(program_id, allocations.collect());

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(program_id),
            vec![],
            PrepareConfig {
                max_pages_override: Some(WasmPagesAmount::UPPER),
                ..Default::default()
            },
        )
    }

    pub fn alloc(repetitions: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = repetitions.checked_mul(API_BENCHMARK_BATCH_SIZE).unwrap();
        let max_pages_to_allocate = MAX_REPETITIONS.checked_mul(2).unwrap();

        assert!(repetitions <= MAX_REPETITIONS);
        // Check that max pages to allocate amount is significantly less than max allocated intervals amount.
        assert!(u16::MAX as u32 > 20 * max_pages_to_allocate);

        // In order to measure the worst case scenario,
        // allocates as many intervals as possible, but leave some place for further allocations:
        // allocations == [1, 3, 5, ..., u16::MAX - max_pages_to_allocate]
        let allocated_amount = (u16::MAX - max_pages_to_allocate as u16) / 2;
        // Add also last possible allocation in order to skip grow calling in bench.
        let allocations = (0..allocated_amount)
            .map(|p| WasmPage::from(p * 2 + 1))
            .chain([WasmPage::UPPER]);

        let mut instructions = vec![Instruction::I32Const(1), Instruction::Call(0)];
        unreachable_condition_i32(&mut instructions, Instruction::I32Eq, -1);

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(0)),
            imported_functions: vec![SyscallName::Alloc],
            handle_body: Some(body::repeated(repetitions, &instructions)),
            ..Default::default()
        };

        Self::prepare_handle_with_allocations(module, allocations)
    }

    pub fn free(repetitions: u32) -> Result<Exec<T>, &'static str> {
        use Instruction::*;

        let repetitions = repetitions.checked_mul(API_BENCHMARK_BATCH_SIZE).unwrap();

        assert!(repetitions <= u16::MAX as u32 / 2 + 1);

        // In order to measure the worst case scenario, allocates as many intervals as possible:
        // allocations == [1, 3, 5, ..., u16::MAX]
        let mut pages: Vec<_> = (0..=u16::MAX / 2).map(|p| p * 2 + 1).collect();
        pages.shuffle(&mut Pcg64::seed_from_u64(1024));

        let mut instructions = vec![];
        for &page in pages.iter().take(repetitions as usize) {
            instructions.extend([I32Const(page as i32), Call(0)]);
            unreachable_condition_i32(&mut instructions, I32Ne, 0);
        }

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(0)),
            imported_functions: vec![SyscallName::Free],
            handle_body: Some(body::from_instructions(instructions)),
            ..Default::default()
        };

        Self::prepare_handle_with_allocations(module, pages.into_iter().map(WasmPage::from))
    }

    pub fn free_range(repetitions: u32, pages: u32) -> Result<Exec<T>, &'static str> {
        use Instruction::*;

        let repetitions = repetitions.checked_mul(API_BENCHMARK_BATCH_SIZE).unwrap();

        assert!(pages > 0);
        assert!(repetitions * pages <= u16::MAX as u32 + 1);

        // In order to measure the worst case scenario, allocates as many intervals as possible:
        // allocations == [1, 3, 5, ..., u16::MAX]
        let allocations = (0..=u16::MAX / 2).map(|p| WasmPage::from(p * 2 + 1));

        let mut numbers: Vec<_> = (0..u16::MAX as u32 / pages).collect();
        numbers.shuffle(&mut Pcg64::seed_from_u64(1024));

        let mut instructions = vec![];
        for start in numbers
            .into_iter()
            .take(repetitions as usize)
            .map(|i| i * pages)
        {
            instructions.extend([
                I32Const(start as i32),
                I32Const((start + pages) as i32),
                Call(0),
            ]);
            unreachable_condition_i32(&mut instructions, I32Ne, 0);
        }

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(0)),
            imported_functions: vec![SyscallName::FreeRange],
            handle_body: Some(body::from_instructions(instructions)),
            ..Default::default()
        };

        Self::prepare_handle_with_allocations(module, allocations.into_iter())
    }

    pub fn gr_reserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r;
        let res_offset = COMMON_OFFSET;

        // It is not allowed to reserve less than mailbox threshold
        let mailbox_threshold = <T as Config>::MailboxThreshold::get();

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SyscallName::ReserveGas],
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
            imported_functions: vec![SyscallName::UnreserveGas],
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
            imported_functions: vec![SyscallName::SystemReserveGas],
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

    pub fn getter(name: SyscallName, r: u32) -> Result<Exec<T>, &'static str> {
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

    pub fn gr_env_vars(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let settings_offset = COMMON_OFFSET;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SyscallName::EnvVars],
            handle_body: Some(body::syscall(
                repetitions,
                &[
                    // version. TODO: Should it be benched based on version?
                    InstrI32Const(1),
                    // offset where to write settings
                    InstrI32Const(settings_offset),
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
            imported_functions: vec![SyscallName::Read],
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
            imported_functions: vec![SyscallName::Read],
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
            imported_functions: vec![SyscallName::Random],
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

    pub fn gr_reply_deposit(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let pid_value_offset = COMMON_OFFSET;
        let send_res_offset = COMMON_OFFSET + PID_VALUE_SIZE;
        let mid_offset = send_res_offset + ERR_LEN_SIZE;
        let res_offset = send_res_offset + ERR_MID_SIZE;

        // `gr_send` is required to populate `message_context.outcome.handle`
        // so `gr_reply_deposit` can be called and won't fail.
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SyscallName::ReplyDeposit, SyscallName::Send],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // pid value offset
                    InstrI32Const(pid_value_offset),
                    // payload offset
                    InstrI32Const(COMMON_OFFSET),
                    // payload len
                    InstrI32Const(0),
                    // delay
                    InstrI32Const(0),
                    // res ptr
                    InstrI32Const(send_res_offset),
                    // call send
                    InstrCall(1),
                    // mid ptr
                    InstrI32Const(mid_offset),
                    // gas
                    InstrI64Const(10_000),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle(module, 10000000)
    }

    pub fn gr_send(
        batches: u32,
        payload_len_kb: Option<u32>,
        wgas: bool,
    ) -> Result<Exec<T>, &'static str> {
        let repetitions = batches * API_BENCHMARK_BATCH_SIZE;
        let pid_value_offset = COMMON_OFFSET;
        let payload_offset = pid_value_offset + PID_VALUE_SIZE;
        let payload_len = payload_len_kb
            .map(kb_to_bytes)
            .unwrap_or(COMMON_PAYLOAD_LEN);
        let res_offset = payload_offset + payload_len;

        let mut params = vec![
            // pid value offset
            InstrI32Const(pid_value_offset),
            // payload offset
            InstrI32Const(payload_offset),
            // payload len
            InstrI32Const(payload_len),
            // delay
            InstrI32Const(10),
        ];

        let name = if wgas {
            params.insert(3, InstrI64Const(100_000_000));
            SyscallName::SendWGas
        } else {
            SyscallName::Send
        };

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![name],
            handle_body: Some(body::fallible_syscall(repetitions, res_offset, &params)),
            ..Default::default()
        };

        Self::prepare_handle(module, 10000000)
    }

    pub fn gr_send_init(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let res_offset = COMMON_OFFSET;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SyscallName::SendInit],
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
            imported_functions: vec![SyscallName::SendPush, SyscallName::SendInit],
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
            imported_functions: vec![SyscallName::SendPush, SyscallName::SendInit],
            handle_body: Some(body::from_instructions(instructions)),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn gr_send_commit(r: u32, wgas: bool) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        assert!(repetitions <= MAX_REPETITIONS);

        let pid_value_offset = COMMON_OFFSET;
        let err_handle_offset = pid_value_offset + PID_VALUE_SIZE;
        let res_offset = err_handle_offset + MAX_REPETITIONS * ERR_HANDLE_SIZE;

        // Init messages
        let mut instructions = body::fallible_syscall_instr(
            MAX_REPETITIONS,
            1,
            Counter(err_handle_offset, ERR_HANDLE_SIZE),
            &[],
        );

        let mut commit_params = vec![
            // get handle from send init results
            Counter(err_handle_offset + ERR_LEN_SIZE, ERR_HANDLE_SIZE),
            InstrI32Load(2, 0),
            // pid value offset
            InstrI32Const(pid_value_offset),
            // delay
            InstrI32Const(10),
        ];
        let name = if wgas {
            commit_params.insert(3, InstrI64Const(100_000_000));
            SyscallName::SendCommitWGas
        } else {
            SyscallName::SendCommit
        };

        instructions.extend(body::fallible_syscall_instr(
            repetitions,
            0,
            InstrI32Const(res_offset),
            &commit_params,
        ));

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![name, SyscallName::SendInit],
            handle_body: Some(body::from_instructions(instructions)),
            ..Default::default()
        };

        Self::prepare_handle(module, 10000000)
    }

    pub fn gr_reservation_send(
        batches: u32,
        payload_len_kb: Option<u32>,
    ) -> Result<Exec<T>, &'static str> {
        let repetitions = batches * API_BENCHMARK_BATCH_SIZE;
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
        let payload_len = payload_len_kb
            .map(kb_to_bytes)
            .unwrap_or(COMMON_PAYLOAD_LEN);
        let res_offset = payload_offset + payload_len;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SyscallName::ReservationSend],
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
        let err_handle_offset = rid_pid_value_offset + rid_pid_values.len() as u32;
        let res_offset = err_handle_offset + MAX_REPETITIONS * ERR_HANDLE_SIZE;

        // Init messages
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
                // rid pid value offset
                Counter(rid_pid_value_offset, RID_PID_VALUE_SIZE),
                // delay
                InstrI32Const(10),
            ],
        ));

        let module = ModuleDefinition {
            // `SMALL_MEM_SIZE + 2` in order to fit data segments and err handle offsets.
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE + 2)),
            imported_functions: vec![SyscallName::ReservationSendCommit, SyscallName::SendInit],
            data_segments: vec![DataSegment {
                offset: rid_pid_value_offset,
                value: rid_pid_values,
            }],
            handle_body: Some(body::from_instructions(instructions)),
            ..Default::default()
        };

        Self::prepare_handle_with_reservation_slots(module, repetitions)
    }

    pub fn gr_reply(
        r: u32,
        payload_len_kb: Option<u32>,
        wgas: bool,
    ) -> Result<Exec<T>, &'static str> {
        let repetitions = r;
        assert!(repetitions <= 1);

        let payload_offset = COMMON_OFFSET;
        let payload_len = payload_len_kb
            .map(kb_to_bytes)
            .unwrap_or(COMMON_PAYLOAD_LEN);
        let value_offset = payload_offset + payload_len;
        let res_offset = value_offset + VALUE_SIZE;

        let mut params = vec![
            // payload offset
            InstrI32Const(payload_offset),
            // payload len
            InstrI32Const(payload_len),
            // value offset
            InstrI32Const(value_offset),
        ];

        let name = match wgas {
            true => {
                params.insert(2, InstrI64Const(100_000_000));
                SyscallName::ReplyWGas
            }
            false => SyscallName::Reply,
        };

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![name],
            handle_body: Some(body::fallible_syscall(repetitions, res_offset, &params)),
            ..Default::default()
        };

        Self::prepare_handle(module, 10000000)
    }

    pub fn gr_reply_commit(r: u32, wgas: bool) -> Result<Exec<T>, &'static str> {
        let repetitions = r;
        assert!(repetitions <= 1);
        let value_offset = COMMON_OFFSET;
        let res_offset = value_offset + VALUE_SIZE;

        let (name, params) = if wgas {
            let params = vec![
                // gas_limit
                InstrI64Const(100_000_000),
                // value offset
                InstrI32Const(value_offset),
            ];

            (SyscallName::ReplyCommitWGas, params)
        } else {
            let params = vec![
                // value offset
                InstrI32Const(value_offset),
            ];

            (SyscallName::ReplyCommit, params)
        };

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![name],
            handle_body: Some(body::fallible_syscall(repetitions, res_offset, &params)),
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
            imported_functions: vec![SyscallName::ReplyPush],
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

        Self::prepare_handle(module, 10_000_000)
    }

    pub fn gr_reply_push_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = 1;
        let payload_offset = COMMON_OFFSET;
        let payload_len = n * 1024;
        let res_offset = payload_offset + payload_len;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SyscallName::ReplyPush],
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

    pub fn gr_reservation_reply(
        batches: u32,
        payload_len_kb: Option<u32>,
    ) -> Result<Exec<T>, &'static str> {
        let repetitions = batches;
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
        let payload_offset = rid_value_offset + rid_values.len() as u32;
        let payload_len = payload_len_kb
            .map(kb_to_bytes)
            .unwrap_or(COMMON_PAYLOAD_LEN);
        let res_offset = payload_offset + payload_len;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SyscallName::ReservationReply],
            data_segments: vec![DataSegment {
                offset: rid_value_offset,
                value: rid_values,
            }],
            handle_body: Some(body::fallible_syscall(
                repetitions,
                res_offset,
                &[
                    // rid value offset
                    Counter(rid_value_offset, RID_VALUE_SIZE),
                    // payload offset
                    InstrI32Const(payload_offset),
                    // payload len
                    InstrI32Const(payload_len),
                ],
            )),
            ..Default::default()
        };

        Self::prepare_handle_with_reservation_slots(module, repetitions)
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
            imported_functions: vec![SyscallName::ReservationReplyCommit],
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
            imported_functions: vec![SyscallName::ReservationReply],
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
            imported_functions: vec![SyscallName::ReplyTo],
            reply_body: Some(body::fallible_syscall(repetitions, res_offset, &[])),
            ..Default::default()
        };

        let instance = Program::<T>::new(module.into(), vec![])?;

        let msg_id = MessageId::from(10);
        let msg = Message::new(
            msg_id,
            instance.addr.as_bytes().try_into().unwrap(),
            ActorId::try_from(instance.caller.clone().into_origin().as_bytes()).unwrap(),
            Default::default(),
            Some(1_000_000),
            0,
            None,
        )
        .into_stored();
        let msg = msg
            .try_into()
            .unwrap_or_else(|_| unreachable!("Signal message sent to user"));
        MailboxOf::<T>::insert(msg, u32::MAX.unique_saturated_into())
            .expect("Error during mailbox insertion");

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Reply(msg_id, ReplyCode::Success(SuccessReplyReason::Manual)),
            vec![],
            Default::default(),
        )
    }

    pub fn gr_signal_code(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let res_offset = COMMON_OFFSET;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SyscallName::SignalCode],
            signal_body: Some(body::fallible_syscall(repetitions, res_offset, &[])),
            ..Default::default()
        };

        Self::prepare_signal_handle(module, 0)
    }

    pub fn gr_signal_from(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let res_offset = COMMON_OFFSET;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SyscallName::SignalFrom],
            signal_body: Some(body::fallible_syscall(repetitions, res_offset, &[])),
            ..Default::default()
        };

        Self::prepare_signal_handle(module, 0)
    }

    pub fn gr_reply_input(
        repetitions: u32,
        input_len_kb: Option<u32>,
        wgas: bool,
    ) -> Result<Exec<T>, &'static str> {
        let input_at = 0;
        let input_len = input_len_kb.map(kb_to_bytes).unwrap_or(COMMON_PAYLOAD_LEN);
        let value_offset = COMMON_OFFSET;
        let res_offset = value_offset + VALUE_SIZE;

        assert!(repetitions <= 1);
        assert!(input_len <= MAX_PAYLOAD_LEN);

        let mut params = vec![
            // input at
            InstrI32Const(input_at),
            // input len
            InstrI32Const(input_len),
            // value offset
            InstrI32Const(value_offset),
        ];

        let name = match wgas {
            true => {
                params.insert(2, InstrI64Const(100_000_000));
                SyscallName::ReplyInputWGas
            }
            false => SyscallName::ReplyInput,
        };

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![name],
            handle_body: Some(body::fallible_syscall(repetitions, res_offset, &params)),
            ..Default::default()
        };

        Self::prepare_handle_with_const_payload(module)
    }

    pub fn gr_reply_push_input(
        batches: Option<u32>,
        input_len_kb: Option<u32>,
    ) -> Result<Exec<T>, &'static str> {
        // We cannot use batches, when big payloads
        assert!(batches.is_some() != input_len_kb.is_some());

        let repetitions = batches
            .map(|batches| batches * API_BENCHMARK_BATCH_SIZE)
            .unwrap_or(1);
        let input_at = 0;
        let input_len = input_len_kb.map(kb_to_bytes).unwrap_or(COMMON_PAYLOAD_LEN);
        let res_offset = COMMON_OFFSET;

        assert!(input_len <= MAX_PAYLOAD_LEN);

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SyscallName::ReplyPushInput],
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

    pub fn gr_send_input(
        batches: u32,
        input_len_kb: Option<u32>,
        wgas: bool,
    ) -> Result<Exec<T>, &'static str> {
        let repetitions = batches * API_BENCHMARK_BATCH_SIZE;
        let input_at = 0;
        let input_len = input_len_kb.map(kb_to_bytes).unwrap_or(COMMON_PAYLOAD_LEN);
        let pid_value_offset = COMMON_OFFSET;
        let res_offset = pid_value_offset + PID_VALUE_SIZE;

        assert!(repetitions <= MAX_REPETITIONS);
        assert!(input_len <= MAX_PAYLOAD_LEN);

        let mut params = vec![
            // pid value offset
            InstrI32Const(pid_value_offset),
            // input at
            InstrI32Const(input_at),
            // input len
            InstrI32Const(input_len),
            // delay
            InstrI32Const(10),
        ];

        let name = match wgas {
            true => {
                params.insert(3, InstrI64Const(100_000_000));
                SyscallName::SendInputWGas
            }
            false => SyscallName::SendInput,
        };

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![name],
            handle_body: Some(body::fallible_syscall(repetitions, res_offset, &params)),
            ..Default::default()
        };

        Self::prepare_handle_with_const_payload(module)
    }

    pub fn gr_send_push_input(r: u32, input_len_kb: Option<u32>) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        assert!(repetitions <= MAX_REPETITIONS);

        let input_at = 0;
        let input_len = input_len_kb.map(kb_to_bytes).unwrap_or(COMMON_PAYLOAD_LEN);
        let res_offset = COMMON_OFFSET;
        let err_handle_offset = COMMON_OFFSET + ERR_LEN_SIZE;

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
                // input at
                InstrI32Const(input_at),
                // input len
                InstrI32Const(input_len),
            ],
        ));

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SyscallName::SendPushInput, SyscallName::SendInit],
            handle_body: Some(body::from_instructions(instructions)),
            ..Default::default()
        };

        Self::prepare_handle_with_const_payload(module)
    }

    pub fn gr_reply_code(r: u32) -> Result<Exec<T>, &'static str> {
        let repetitions = r * API_BENCHMARK_BATCH_SIZE;
        let res_offset = COMMON_OFFSET;

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(SMALL_MEM_SIZE)),
            imported_functions: vec![SyscallName::ReplyCode],
            reply_body: Some(body::fallible_syscall(repetitions, res_offset, &[])),
            ..Default::default()
        };

        let instance = Program::<T>::new(module.into(), vec![])?;

        let msg_id = MessageId::from(10);
        let msg = Message::new(
            msg_id,
            instance.addr.as_bytes().try_into().unwrap(),
            ActorId::try_from(instance.caller.clone().into_origin().as_bytes()).unwrap(),
            Default::default(),
            Some(1_000_000),
            0,
            None,
        )
        .into_stored();
        let msg = msg
            .try_into()
            .unwrap_or_else(|_| unreachable!("Signal message sent to user"));
        MailboxOf::<T>::insert(msg, u32::MAX.unique_saturated_into())
            .expect("Error during mailbox insertion");

        utils::prepare_exec::<T>(
            instance.caller.into_origin(),
            HandleKind::Reply(msg_id, ReplyCode::Success(SuccessReplyReason::Manual)),
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
            imported_functions: vec![SyscallName::Debug],
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
            imported_functions: vec![SyscallName::Debug],
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

    pub fn termination_bench(
        name: SyscallName,
        param: Option<u32>,
        r: u32,
    ) -> Result<Exec<T>, &'static str> {
        let repetitions = r;
        assert!(repetitions <= 1);

        let params = if let Some(c) = param {
            assert!(name.signature().params().len() == 1);
            vec![InstrI32Const(c)]
        } else {
            assert!(name.signature().params().is_empty());
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
            imported_functions: vec![SyscallName::Wake],
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

    pub fn gr_create_program(
        batches: u32,
        payload_len_kb: Option<u32>,
        salt_len_kb: Option<u32>,
        wgas: bool,
    ) -> Result<Exec<T>, &'static str> {
        let repetitions = batches * API_BENCHMARK_BATCH_SIZE;

        let module = WasmModule::<T>::dummy();
        let _ = Gear::<T>::upload_code_raw(module.code);

        let mut cid_value = [0; CID_VALUE_SIZE as usize];
        cid_value[0..CID_SIZE as usize].copy_from_slice(module.hash.as_ref());
        cid_value[CID_SIZE as usize..].copy_from_slice(&0u128.to_le_bytes());

        let cid_value_offset = COMMON_OFFSET;
        let payload_offset = cid_value_offset + cid_value.len() as u32;
        let payload_len = payload_len_kb.map(kb_to_bytes).unwrap_or(10);
        let res_offset = payload_offset + payload_len;

        // Use previous result bytes as part of salt buffer. First one uses 0 bytes.
        let salt_offset = res_offset;
        let salt_len = salt_len_kb.map(kb_to_bytes).unwrap_or(32);

        let mut params = vec![
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
            // delay
            InstrI32Const(10),
        ];

        let name = match wgas {
            true => {
                params.insert(5, InstrI64Const(100_000_000));
                SyscallName::CreateProgramWGas
            }
            false => SyscallName::CreateProgram,
        };

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![name],
            data_segments: vec![DataSegment {
                offset: cid_value_offset,
                value: cid_value.to_vec(),
            }],
            handle_body: Some(body::fallible_syscall(repetitions, res_offset, &params)),
            ..Default::default()
        };

        Self::prepare_handle(module, 0)
    }

    pub fn lazy_pages_signal_read(end_page: WasmPage) -> Result<Exec<T>, &'static str> {
        let instrs = body::read_access_all_pages_instrs(end_page, vec![]);
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            stack_end: Some(0.into()),
            ..Default::default()
        };
        Self::prepare_handle(module, 0)
    }

    pub fn lazy_pages_signal_write(end_page: WasmPage) -> Result<Exec<T>, &'static str> {
        let instrs = body::write_access_all_pages_instrs(end_page, vec![]);
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            stack_end: Some(0.into()),
            ..Default::default()
        };
        Self::prepare_handle(module, 0)
    }

    pub fn lazy_pages_signal_write_after_read(
        end_page: WasmPage,
        max_page: WasmPage,
    ) -> Result<Exec<T>, &'static str> {
        let instrs = body::read_access_all_pages_instrs(max_page, vec![]);
        let instrs = body::write_access_all_pages_instrs(end_page, instrs);
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            stack_end: Some(0.into()),
            ..Default::default()
        };
        Self::prepare_handle(module, 0)
    }

    pub fn lazy_pages_load_page_storage_data(end_page: WasmPage) -> Result<Exec<T>, &'static str> {
        let exec = Self::lazy_pages_signal_read(end_page)?;
        let program_id = exec.context.program_id();
        IntervalIterator::from(..end_page)
            .flat_map(|p: WasmPage| p.to_iter())
            .for_each(|page: GearPage| {
                ProgramStorageOf::<T>::set_program_page_data(
                    program_id,
                    exec.context.memory_infix(),
                    page,
                    PageBuf::from_inner(PageBufInner::filled_with(1)),
                );
            });
        Ok(exec)
    }

    pub fn lazy_pages_host_func_read(end_page: WasmPage) -> Result<Exec<T>, &'static str> {
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SyscallName::Debug],
            handle_body: Some(body::from_instructions(vec![
                // payload offset
                Instruction::I32Const(0),
                // payload len
                Instruction::I32Const(end_page.offset() as i32),
                // CALL
                Instruction::Call(0),
            ])),
            stack_end: Some(0.into()),
            ..Default::default()
        };
        Self::prepare_handle(module, 0)
    }

    pub fn lazy_pages_host_func_write(end_page: WasmPage) -> Result<Exec<T>, &'static str> {
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SyscallName::Read],
            handle_body: Some(body::from_instructions(vec![
                // at
                Instruction::I32Const(0),
                // len
                Instruction::I32Const(end_page.offset() as i32),
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
        end_page: WasmPage,
    ) -> Result<Exec<T>, &'static str> {
        let max_page = WasmPage::from_offset(MAX_PAYLOAD_LEN);
        assert!(end_page <= max_page);

        // Access const amount of pages before `gr_read` calls in order to make all pages read accessed.
        let mut instrs = body::read_access_all_pages_instrs(max_page, vec![]);

        // Add `gr_read` call.
        instrs.extend_from_slice(&[
            // at
            Instruction::I32Const(0),
            // len
            Instruction::I32Const(end_page.offset() as i32),
            // buffer ptr
            Instruction::I32Const(0),
            // err len ptr
            Instruction::I32Const(0),
            // CALL
            Instruction::Call(0),
        ]);

        let module = ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SyscallName::Read],
            handle_body: Some(body::from_instructions(instrs)),
            stack_end: Some(0.into()),
            ..Default::default()
        };

        Self::prepare_handle_with_const_payload(module)
    }
}
