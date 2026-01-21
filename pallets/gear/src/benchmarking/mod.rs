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

//! Benchmarks for the gear pallet
//!
//! ## i32const benchmarking
//! Wasmer has many optimizations, that optimize i32const usage,
//! so calculate this instruction constant weight is not easy.
//! Because of this we suppose that i32const instruction has weight = 0,
//! in cases we subtract its weight from benchmark weight to calculate
//! benched instruction weight. But also we suppose i32const == i64const,
//! when we calculate block code weight. This is more safe solution,
//! but also more expensive.
//!
//! ## Drop, Block, End
//! This is virtual instruction for wasmer, they aren't really generated in target code,
//! the only thing they do - wasmer take them in account, when compiles wasm code.
//! So, we suppose this instruction have weight 0.

#[allow(dead_code)]
mod code;
mod sandbox;

mod syscalls;
mod tasks;
mod utils;
use syscalls::Benches;

mod tests;
use tests::syscalls_integrity;

use self::{
    code::{
        ImportedMemory, Location, ModuleDefinition, OFFSET_AUX, TableSegment, WasmModule,
        body::{self, DynInstr::*},
        max_pages,
    },
    sandbox::Sandbox,
};
use crate::{
    BalanceOf, BenchmarkStorage, BlockNumberFor, Call, Config, CurrencyOf, Event, Ext,
    GasHandlerOf, GearBank, MailboxOf, Pallet as Gear, Pallet, ProgramStorageOf, QueueOf, Schedule,
    TaskPoolOf,
    builtin::BuiltinDispatcherFactory,
    manager::ExtManager,
    pallet,
    schedule::{API_BENCHMARK_BATCH_SIZE, INSTR_BENCHMARK_BATCH_SIZE},
};
use ::alloc::{collections::BTreeMap, vec};
use common::{
    self, CodeStorage, GasTree, Origin, ProgramStorage, ReservableTree, benchmarking,
    storage::{Counter, *},
};
use core_processor::{
    ProcessExecutionContext, ProcessorContext, ProcessorExternalities,
    common::{DispatchOutcome, JournalNote},
    configs::BlockConfig,
};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::{Currency, Get, Hooks};
use frame_system::{Pallet as SystemPallet, RawOrigin};
use gear_core::{
    buffer::Payload,
    code::{Code, CodeAndId},
    ids::{ActorId, CodeId, MessageId, prelude::*},
    memory::Memory,
    message::{DispatchKind, Salt},
    pages::{WasmPage, WasmPagesAmount},
    program::ActiveProgram,
    tasks::{ScheduledTask, TaskHandler},
};
use gear_core_backend::{
    env::Environment,
    memory::{BackendMemory, ExecutorMemory},
    mock::MockExt,
    state::HostState,
};
use gear_core_errors::*;
use gear_sandbox::{SandboxMemory, SandboxStore, default_executor::Store};
use gear_wasm_instrument::{
    BlockType, BrTable, Instruction, MemArg, ValType, syscalls::SyscallName,
};
use pallet_authorship::Pallet as AuthorshipPallet;
use parity_scale_codec::Encode;
use sp_consensus_babe::{
    BABE_ENGINE_ID, Slot,
    digests::{PreDigest, SecondaryPlainPreDigest},
};
use sp_core::H256;
use sp_runtime::{
    Digest, DigestItem, Perbill, Saturating,
    traits::{Bounded, CheckedAdd, One, UniqueSaturatedInto, Zero},
};
use sp_std::{num::NonZero, prelude::*};

const MAX_PAYLOAD_LEN: u32 = Payload::MAX_LEN as u32;
const MAX_PAYLOAD_LEN_KB: u32 = MAX_PAYLOAD_LEN / 1024;
const MAX_SALT_SIZE_BYTES: u32 = Salt::MAX_LEN as u32;
const MAX_NUMBER_OF_DATA_SEGMENTS: u32 = 1024;
const MAX_TABLE_ENTRIES: u32 = 10_000_000;

/// How many batches we do per API benchmark.
const API_BENCHMARK_BATCHES: u32 = 20;

/// How many batches we do per Instruction benchmark.
const INSTR_BENCHMARK_BATCHES: u32 = 50;

/// Default memory size in wasm pages for benchmarks.
const DEFAULT_MEM_SIZE: u16 = 512;

// Initializes new block.
fn init_block<T: Config>(previous: Option<BlockNumberFor<T>>)
where
    T::AccountId: Origin,
{
    // All blocks are to be authored by validator at index 0
    let slot = Slot::from(0);
    let pre_digest = Digest {
        logs: vec![DigestItem::PreRuntime(
            BABE_ENGINE_ID,
            PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
                slot,
                authority_index: 0,
            })
            .encode(),
        )],
    };

    let bn = previous
        .unwrap_or_else(Zero::zero)
        .checked_add(&One::one())
        .expect("overflow");

    SystemPallet::<T>::initialize(&bn, &SystemPallet::<T>::parent_hash(), &pre_digest);
    SystemPallet::<T>::set_block_number(bn);
    SystemPallet::<T>::on_initialize(bn);
    AuthorshipPallet::<T>::on_initialize(bn);
}

// Initializes block and runs queue processing.
fn process_queue<T: Config>()
where
    T::AccountId: Origin,
{
    init_block::<T>(None);

    let (builtins, _) = T::BuiltinDispatcherFactory::create();
    let ext_manager = ExtManager::<T>::new(builtins);
    Gear::<T>::process_queue(ext_manager);
}

fn verify_process(notes: Vec<JournalNote>) {
    assert!(
        !notes.is_empty(),
        "Journal notes cannot be empty after execution"
    );

    let mut pages_data = BTreeMap::new();

    for note in notes {
        match note {
            JournalNote::MessageDispatched {
                outcome: DispatchOutcome::InitFailure { .. } | DispatchOutcome::MessageTrap { .. },
                ..
            } => {
                panic!("Process was not successful")
            }
            JournalNote::UpdatePage {
                page_number, data, ..
            } => {
                pages_data.insert(page_number, data);
            }
            _ => {}
        }
    }
}

fn run_process<T>(exec: Exec<T>) -> Vec<JournalNote>
where
    T: Config,
    T::AccountId: Origin,
{
    core_processor::process::<Ext>(&exec.block_config, exec.context, exec.random_data)
        .unwrap_or_else(|e| unreachable!("core-processor logic invalidated: {}", e))
}

pub fn find_latest_event<T, F, R>(mapping_filter: F) -> Option<R>
where
    T: Config,
    F: Fn(Event<T>) -> Option<R>,
{
    SystemPallet::<T>::events()
        .into_iter()
        .rev()
        .filter_map(|event_record| {
            let event = <<T as pallet::Config>::RuntimeEvent as From<_>>::from(event_record.event);
            let event: Result<Event<T>, _> = event.try_into();

            event.ok()
        })
        .find_map(mapping_filter)
}

/// An instantiated and deployed program.
#[derive(Clone)]
struct Program<T: Config> {
    addr: H256,
    caller: T::AccountId,
}

impl<T: Config> Program<T>
where
    T: Config,
    T::AccountId: Origin,
{
    /// Create new program and use a default account id as instantiator.
    fn new(module: WasmModule<T>, data: Vec<u8>) -> Result<Program<T>, &'static str> {
        Self::with_index(0, module, data)
    }

    /// Create new program and use an account id derived from the supplied index as instantiator.
    fn with_index(
        index: u32,
        module: WasmModule<T>,
        data: Vec<u8>,
    ) -> Result<Program<T>, &'static str> {
        Self::with_caller(
            benchmarking::account("instantiator", index, 0),
            module,
            data,
        )
    }

    /// Create new program and use the supplied `caller` as instantiator.
    fn with_caller(
        caller: T::AccountId,
        module: WasmModule<T>,
        data: Vec<u8>,
    ) -> Result<Program<T>, &'static str> {
        // In case of the `gr_create_program` syscall testing, we can have as many as
        // `API_BENCHMARK_BATCHES * API_BENCHMARK_BATCH_SIZE` repetitions of it in a module,
        // which requires a transfer of the ED each time the syscall is called.
        // For the above to always succeed, we need to ensure the contract has enough funds.
        let value = CurrencyOf::<T>::minimum_balance()
            .saturating_mul((API_BENCHMARK_BATCHES * API_BENCHMARK_BATCH_SIZE).into());
        CurrencyOf::<T>::make_free_balance_be(&caller, caller_funding::<T>());
        let salt = vec![0xff];
        let addr = ActorId::generate_from_user(module.hash, &salt).into_origin();

        Gear::<T>::upload_program_raw(
            RawOrigin::Signed(caller.clone()).into(),
            module.code,
            salt,
            data,
            250_000_000_000,
            value,
        )?;

        process_queue::<T>();

        let result = Program { caller, addr };

        Ok(result)
    }
}

/// The funding that each account that either calls or instantiates programs is funded with.
fn caller_funding<T: pallet::Config>() -> BalanceOf<T> {
    BalanceOf::<T>::max_value() / 2u32.into()
}

pub struct Exec<T: Config> {
    #[allow(unused)]
    ext_manager: ExtManager<T>,
    block_config: BlockConfig,
    context: ProcessExecutionContext,
    random_data: (Vec<u8>, u32),
}

benchmarks! {

    where_clause { where
        T::AccountId: Origin,
        T: pallet_gear_voucher::Config,
    }

    #[extra]
    read_big_state {
        syscalls_integrity::read_big_state::<T>();
    } : {}

    #[extra]
    signal_stack_limit_exceeded_works {
        syscalls_integrity::signal_stack_limit_exceeded_works::<T>();
    } : {}

    #[extra]
    check_all {
        syscalls_integrity::main_test::<T>();
        tests::check_stack_overflow::<T>();

        tests::lazy_pages::lazy_pages_charging::<T>();
        tests::lazy_pages::lazy_pages_charging_special::<T>();
        tests::lazy_pages::lazy_pages_gas_exceed::<T>();
    } : {}

    #[extra]
    check_stack_overflow {
        tests::check_stack_overflow::<T>();
    }: {}

    #[extra]
    check_lazy_pages_all {
        tests::lazy_pages::lazy_pages_charging::<T>();
        tests::lazy_pages::lazy_pages_charging_special::<T>();
        tests::lazy_pages::lazy_pages_gas_exceed::<T>();
    } : {}

    #[extra]
    check_syscalls_integrity {
        syscalls_integrity::main_test::<T>();
    }: {}

    #[extra]
    check_lazy_pages_charging {
        tests::lazy_pages::lazy_pages_charging::<T>();
    }: {}

    #[extra]
    check_lazy_pages_charging_special {
        tests::lazy_pages::lazy_pages_charging_special::<T>();
    }: {}

    #[extra]
    check_lazy_pages_gas_exceed {
        tests::lazy_pages::lazy_pages_gas_exceed::<T>();
    }: {}

    // This bench uses `StorageMap` as a storage, due to the fact that
    // the most of the gear storages represented with this type.
    db_write_per_kb {
        // Code is the biggest data could be written into storage in gear runtime.
        let c in 0 .. T::Schedule::get().limits.code_len / 1024;

        // Data to be written.
        let data = vec![c as u8; 1024 * c as usize];
    }: {
        // Inserting data into the storage.
        BenchmarkStorage::<T>::insert(c, data);
    }

    // This bench uses `StorageMap` as a storage, due to the fact that
    // the most of the gear storages represented with this type.
    db_read_per_kb {
        // Code is the biggest data could be written into storage in gear runtime.
        let c in 0 .. T::Schedule::get().limits.code_len / 1024;

        // Data to be queried further.
        let data = vec![c as u8; 1024 * c as usize];

        // Placing data in storage to be able to query it.
        BenchmarkStorage::<T>::insert(c, data);
    }: {
        // Querying data from storage.
        BenchmarkStorage::<T>::get(c).expect("Infallible: Key not found in storage");
    }

    // `c`: Size of the code section in kilobytes.
    instantiate_module_code_section_per_kb {
        let c in 0 .. T::Schedule::get().limits.code_len / 1024;

        let WasmModule { code, .. } = WasmModule::<T>::sized(c * 1024, Location::Init);
        let ext = Ext::new(ProcessorContext::new_mock());
    }: {
        Environment::new(ext, &code, Default::default(), max_pages::<T>().into(), |_,_,_|{}).unwrap();
    }

    // `d`: Size of the data section in kilobytes.
    instantiate_module_data_section_per_kb {
        let d in 0 .. T::Schedule::get().limits.code_len / 1024;

        let WasmModule { code, .. } = WasmModule::<T>::sized_data_section(d * 1024, MAX_NUMBER_OF_DATA_SEGMENTS);
        let ext = Ext::new(ProcessorContext::new_mock());
    }: {
        Environment::new(ext, &code, Default::default(), max_pages::<T>().into(),|_,_,_|{}).unwrap();
    }

    // `g`: Size of the global section in kilobytes.
    instantiate_module_global_section_per_kb {
        let g in 0 .. T::Schedule::get().limits.code_len / 1024;

        let WasmModule { code, .. } = WasmModule::<T>::sized_global_section(g * 1024);
        let ext = Ext::new(ProcessorContext::new_mock());
    }: {
        Environment::new(ext, &code, Default::default(), max_pages::<T>().into(),|_,_,_|{}).unwrap();
    }

    // `t`: Size of the memory allocated for the table after instantiation, in kilobytes.
    instantiate_module_table_section_per_kb {
        let t in 0 .. MAX_TABLE_ENTRIES / 1024;

        let WasmModule { code, .. } = WasmModule::<T>::sized_table_section(t * 1024, None);
        let ext = Ext::new(ProcessorContext::new_mock());
    }: {
        Environment::new(ext, &code, Default::default(), max_pages::<T>().into(),|_,_,_|{}).unwrap();
    }

    // `e`: Size of the element section in kilobytes.
    instantiate_module_element_section_per_kb {
        let e in 0 .. T::Schedule::get().limits.code_len / 1024;

        let max_table_size = T::Schedule::get().limits.code_len;
        let WasmModule { code, .. } = WasmModule::<T>::sized_table_section(max_table_size, Some(e * 1024));
        let ext = Ext::new(ProcessorContext::new_mock());
    }: {
        Environment::new(ext, &code, Default::default(), max_pages::<T>().into(), |_,_,_|{}).unwrap();
    }

    // `t`: Size of the type section in kilobytes.
    instantiate_module_type_section_per_kb {
        let t in 0 .. T::Schedule::get().limits.type_section_len / 1024;

        let WasmModule { code, .. } = WasmModule::<T>::sized_type_section(t * 1024);
        let ext = Ext::new(ProcessorContext::new_mock());
    }: {
        Environment::new(ext, &code, Default::default(), max_pages::<T>().into(), |_,_,_|{}).unwrap();
    }

    claim_value {
        let caller = benchmarking::account("caller", 0, 0);
        let _ = CurrencyOf::<T>::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let program_id = benchmarking::account::<T::AccountId>("program", 0, 100);
        let _ = CurrencyOf::<T>::deposit_creating(&program_id, 100_000_000_000_000_u128.unique_saturated_into());
        let code = benchmarking::generate_wasm(16.into()).unwrap();
        benchmarking::set_program::<ProgramStorageOf::<T>, _>(program_id.clone().cast(), code);
        let original_message_id = benchmarking::account::<T::AccountId>("message", 0, 100).cast();
        let gas_limit = 50000;
        let value = 10000u32.into();
        let multiplier = <T as pallet_gear_bank::Config>::GasMultiplier::get();
        GasHandlerOf::<T>::create(program_id.clone(), multiplier, original_message_id, gas_limit).expect("Failed to create gas handler");
        GearBank::<T>::deposit_gas(&program_id, gas_limit, true).unwrap_or_else(|e| unreachable!("Gear bank error: {e:?}"));
        GearBank::<T>::deposit_value(&program_id, value, true).unwrap_or_else(|e| unreachable!("Gear bank error: {e:?}"));
        MailboxOf::<T>::insert(gear_core::message::StoredMessage::new(
            original_message_id,
            program_id.cast(),
            caller.clone().cast(),
            Default::default(),
            value.unique_saturated_into(),
            None,
        ).try_into().unwrap_or_else(|_| unreachable!("Signal message sent to user")), u32::MAX.unique_saturated_into()).expect("Error during mailbox insertion");

        init_block::<T>(None);
    }: _(RawOrigin::Signed(caller.clone()), original_message_id)
    verify {
        let auto_reply = QueueOf::<T>::dequeue().expect("Error in algorithm").expect("Element should be");
        assert!(auto_reply.payload_bytes().is_empty());
        assert_eq!(auto_reply.reply_details().expect("Should be").to_reply_code(), ReplyCode::Success(SuccessReplyReason::Auto));
        assert!(MailboxOf::<T>::is_empty(&caller));
    }

    // This constructs a program that is maximal expensive to instrument.
    // It creates a maximum number of metering blocks per byte.
    //
    // `c`: Size of the code in kilobytes.
    upload_code {
        let c in 0 .. Perbill::from_percent(49).mul_ceil(T::Schedule::get().limits.code_len) / 1024;
        let value = CurrencyOf::<T>::minimum_balance();
        let caller = whitelisted_caller();
        CurrencyOf::<T>::make_free_balance_be(&caller, caller_funding::<T>());
        let WasmModule { code, hash: code_id, .. } = WasmModule::<T>::sized(c * 1024, Location::Handle);
        let origin = RawOrigin::Signed(caller);

        init_block::<T>(None);
    }: _(origin, code)
    verify {
        assert!(<T as pallet::Config>::CodeStorage::original_code_exists(code_id));
        assert!(<T as pallet::Config>::CodeStorage::instrumented_code_exists(code_id));
    }

    // The size of the salt influences the runtime because it is hashed in order to
    // determine the program address.
    //
    // `s`: Size of the salt in bytes.
    create_program {
        let s in 0 .. MAX_SALT_SIZE_BYTES;
        let p in 0 .. MAX_PAYLOAD_LEN;

        let caller = whitelisted_caller();
        let origin = RawOrigin::Signed(caller);

        let WasmModule { code, hash: code_id, .. } = WasmModule::<T>::dummy();
        Gear::<T>::upload_code(origin.into(), code).expect("submit code failed");

        let salt = vec![42u8; s as usize];
        let init_payload = vec![42u8; p as usize];
        let value = CurrencyOf::<T>::minimum_balance();
        let caller = whitelisted_caller();
        CurrencyOf::<T>::make_free_balance_be(&caller, caller_funding::<T>());
        let origin = RawOrigin::Signed(caller);

        init_block::<T>(None);
    }: _(origin, code_id, salt, init_payload, 100_000_000_u64, value, false)
    verify {
        assert!(<T as pallet::Config>::CodeStorage::original_code_exists(code_id));
        assert!(<T as pallet::Config>::CodeStorage::instrumented_code_exists(code_id));
    }

    // This constructs a program that is maximal expensive to instrument.
    // It creates a maximum number of metering blocks per byte.
    // The size of the salt influences the runtime because is is hashed in order to
    // determine the program address.
    //
    // `c`: Size of the code in kilobytes.
    // `s`: Size of the salt in bytes.
    //
    // # Note
    //
    // We cannot let `c` grow to the maximum code size because the code is not allowed
    // to be larger than the maximum size **after instrumentation**.
    upload_program {
        let c in 0 .. Perbill::from_percent(49).mul_ceil(T::Schedule::get().limits.code_len) / 1024;
        let s in 0 .. MAX_SALT_SIZE_BYTES;
        let p in 0 .. MAX_PAYLOAD_LEN;
        let salt = vec![42u8; s as usize];
        let init_payload = vec![42u8; p as usize];
        let value = CurrencyOf::<T>::minimum_balance();
        let caller = whitelisted_caller();
        CurrencyOf::<T>::make_free_balance_be(&caller, caller_funding::<T>());
        let WasmModule { code, hash, .. } = WasmModule::<T>::sized(c * 1024, Location::Handle);
        let origin = RawOrigin::Signed(caller);

        init_block::<T>(None);
    }: _(origin, code, salt, init_payload, 100_000_000_u64, value, false)
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(Some(_))));
    }

    send_message {
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller = benchmarking::account("caller", 0, 0);
        let _ = CurrencyOf::<T>::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let minimum_balance = CurrencyOf::<T>::minimum_balance();
        let program_id = benchmarking::account::<T::AccountId>("program", 0, 100).cast();
        let code = benchmarking::generate_wasm(16.into()).unwrap();
        benchmarking::set_program::<ProgramStorageOf::<T>, _>(program_id, code);
        let payload = vec![0_u8; p as usize];

        init_block::<T>(None);
    }: _(RawOrigin::Signed(caller), program_id, payload, 100_000_000_u64, minimum_balance, false)
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(Some(_))));
    }

    send_reply {
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller = benchmarking::account("caller", 0, 0);
        let _ = CurrencyOf::<T>::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let minimum_balance = CurrencyOf::<T>::minimum_balance();
        let program_id = benchmarking::account::<T::AccountId>("program", 0, 100);
        let _ = CurrencyOf::<T>::deposit_creating(&program_id, 100_000_000_000_000_u128.unique_saturated_into());
        let code = benchmarking::generate_wasm(16.into()).unwrap();
        benchmarking::set_program::<ProgramStorageOf::<T>, _>(program_id.clone().cast(), code);
        let original_message_id = benchmarking::account::<T::AccountId>("message", 0, 100).cast();
        let gas_limit = 50000;
        let value = (p % 2).into();
        let multiplier = <T as pallet_gear_bank::Config>::GasMultiplier::get();
        GasHandlerOf::<T>::create(program_id.clone(), multiplier, original_message_id, gas_limit).expect("Failed to create gas handler");
        GearBank::<T>::deposit_gas(&program_id, gas_limit, true).unwrap_or_else(|e| unreachable!("Gear bank error: {e:?}"));
        GearBank::<T>::deposit_value(&program_id, value, true).unwrap_or_else(|e| unreachable!("Gear bank error: {e:?}"));
        MailboxOf::<T>::insert(gear_core::message::StoredMessage::new(
            original_message_id,
            program_id.cast(),
            caller.clone().cast(),
            Default::default(),
            value.unique_saturated_into(),
            None,
        ).try_into().unwrap_or_else(|_| unreachable!("Signal message sent to user")), u32::MAX.unique_saturated_into()).expect("Error during mailbox insertion");
        let payload = vec![0_u8; p as usize];

        init_block::<T>(None);
    }: _(RawOrigin::Signed(caller.clone()), original_message_id, payload, 100_000_000_u64, minimum_balance, false)
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(Some(_))));
        assert!(MailboxOf::<T>::is_empty(&caller))
    }

    claim_value_to_inheritor {
        let d in 1 .. 1024;

        let minimum_balance = CurrencyOf::<T>::minimum_balance();

        let caller: T::AccountId = benchmarking::account("caller", 0, 0);

        let mut inheritor = caller.clone().cast();
        let mut programs = vec![];
        for i in 0..d {
            let program_id = benchmarking::account::<T::AccountId>("program", i, 100);
            programs.push(program_id.clone());
            let _ = CurrencyOf::<T>::deposit_creating(&program_id, minimum_balance);
            let program_id = program_id.cast();
            benchmarking::set_program::<ProgramStorageOf::<T>, _>(program_id, vec![]);

            ProgramStorageOf::<T>::update_program_if_active(program_id, |program, _bn| {
                if i.is_multiple_of(2) {
                    *program = common::Program::Terminated(inheritor);
                } else {
                    *program = common::Program::Exited(inheritor);
                }
            })
            .unwrap();

            inheritor = program_id;
        }

        let program_id = inheritor;

        init_block::<T>(None);
    }: _(RawOrigin::Signed(caller.clone()), program_id, NonZero::<u32>::MAX)
    verify {
        assert_eq!(
            CurrencyOf::<T>::free_balance(&caller),
            minimum_balance * d.unique_saturated_into()
        );

        for program_id in programs {
            assert_eq!(CurrencyOf::<T>::free_balance(&program_id), BalanceOf::<T>::zero());
        }
    }

    // This benchmarks the additional weight that is charged when a program is executed the
    // first time after a new schedule was deployed: For every new schedule a program needs
    // to re-run the instrumentation once.
    reinstrument_per_kb {
        let e in 0 .. T::Schedule::get().limits.code_len / 1_024;

        let max_table_size = T::Schedule::get().limits.code_len;
        // NOTE: We use a program filled with table/element sections here because it is the heaviest weight-wise.
        let WasmModule { code, hash, .. } = WasmModule::<T>::sized_table_section(max_table_size, Some(e * 1024));
        let code = Code::try_new_mock_const_or_no_rules(code, false, Default::default()).unwrap();
        let code_and_id = CodeAndId::new(code);

        T::CodeStorage::add_code(code_and_id.clone()).unwrap();

        let (code, code_id) = code_and_id.into_parts();
        let (_, _, code_metadata) = code.into_parts();

        let schedule = T::Schedule::get();
    }: {
        Gear::<T>::reinstrument_code(code_id, code_metadata, &schedule).expect("Re-instrumentation  failed");
    }

    load_allocations_per_interval {
        let a in 0 .. u16::MAX as u32 / 2;
        let allocations = (0..a).map(|p| WasmPage::from(p as u16 * 2 + 1));
        let program_id = benchmarking::account::<T::AccountId>("program", 0, 100).cast();
        let code = benchmarking::generate_wasm(16.into()).unwrap();
        benchmarking::set_program::<ProgramStorageOf::<T>, _>(program_id, code);
        ProgramStorageOf::<T>::set_allocations(program_id, allocations.collect());
    }: {
        let _ = ProgramStorageOf::<T>::allocations(program_id).unwrap();
    }

    alloc {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::alloc(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    mem_grow {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut store = Store::<HostState<MockExt, BackendMemory<ExecutorMemory>>>::new(None);
        let mem = ExecutorMemory::new(&mut store, 1, None).unwrap();
        let mem = BackendMemory::from(mem);
    }: {
        for _ in 0..(r * API_BENCHMARK_BATCH_SIZE) {
            mem.grow(&mut store, 1.into()).unwrap();
        }

    }

    mem_grow_per_page {
        let p in 1 .. u32::from(WasmPagesAmount::UPPER) / API_BENCHMARK_BATCH_SIZE;
        let mut store = Store::<HostState<MockExt, BackendMemory<ExecutorMemory>>>::new(None);
        let mem = ExecutorMemory::new(&mut store, 1, None).unwrap();
        let mem = BackendMemory::from(mem);
    }: {
        for _ in 0..API_BENCHMARK_BATCH_SIZE {
            mem.grow(&mut store, (p as u16).into()).unwrap();
        }
    }

    free {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::free(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    free_range {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::free_range(r, 1)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    free_range_per_page {
        let p in 1 .. 700;
        let mut res = None;
        let exec = Benches::<T>::free_range(1, p)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reserve_gas {
        let r in 0 .. T::ReservationsLimit::get() as u32;
        let mut res = None;
        let exec = Benches::<T>::gr_reserve_gas(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_unreserve_gas {
        let r in 0 .. T::ReservationsLimit::get() as u32;
        let mut res = None;
        let exec = Benches::<T>::gr_unreserve_gas(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_system_reserve_gas {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_system_reserve_gas(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_message_id {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SyscallName::MessageId, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_program_id {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SyscallName::ProgramId, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_source {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SyscallName::Source, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_value {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SyscallName::Value, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_value_available {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SyscallName::ValueAvailable, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_gas_available {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SyscallName::GasAvailable, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_size {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SyscallName::Size, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_read {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_read(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_read_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_read_per_kb(n)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_env_vars {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_env_vars(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_block_height {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SyscallName::BlockHeight, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_block_timestamp {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SyscallName::BlockTimestamp, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_random {
        let n in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_random(n)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reply_deposit {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_deposit(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_send(r, None, false)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_send(1, Some(n), false)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_wgas {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_send(r, None, true)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_wgas_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_send(1, Some(n), true)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_input {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_send_input(r, None, false)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_input_wgas {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_send_input(r, None, true)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_init {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_send_init(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_push {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_send_push(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_push_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_send_push_per_kb(n)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_commit {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_send_commit(r, false)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_commit_wgas {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_send_commit(r, true)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reservation_send {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_reservation_send(r, None)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reservation_send_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_reservation_send(1, Some(n))?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reservation_send_commit {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_reservation_send_commit(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_reply` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_reply {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::gr_reply(r, None, false)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reply_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_reply(1, Some(n), false)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_reply_wgas` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_reply_wgas {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::gr_reply(r, None, true)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reply_wgas_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_reply(1, Some(n), true)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_reply_commit` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_reply_commit {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_commit(r, false)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_reply_commit_wgas` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_reply_commit_wgas {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_commit(r, true)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reply_push {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_push(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reply_push_per_kb {
        let n in 0 .. gear_core::buffer::MAX_PAYLOAD_SIZE as u32 / 1024;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_push_per_kb(n)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_reply_input` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_reply_input {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_input(r, None, false)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_reply_input_wgas` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_reply_input_wgas {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_input(r, None, true)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_reservation_reply` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_reservation_reply {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::gr_reservation_reply(r, None)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reservation_reply_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_reservation_reply(1, Some(n))?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_reservation_reply_commit` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_reservation_reply_commit {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::gr_reservation_reply_commit(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reservation_reply_commit_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_reservation_reply_commit_per_kb(n)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reply_to {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_to(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_signal_code {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_signal_code(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_signal_from {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_signal_from(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reply_push_input {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_push_input(Some(r), None)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reply_push_input_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_push_input(None, Some(n))?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_push_input {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_send_push_input(r, None)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_push_input_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_send_push_input(1, Some(n))?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_debug {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_debug(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_debug_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_debug_per_kb(n)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reply_code {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_code(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_exit` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_exit {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::termination_bench(SyscallName::Exit, Some(0xff), r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_leave` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_leave {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::termination_bench(SyscallName::Leave, None, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_wait` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_wait {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::termination_bench(SyscallName::Wait, None, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_wait_for` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_wait_for {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::termination_bench(SyscallName::WaitFor, Some(10), r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We cannot call `gr_wait_up_to` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_wait_up_to {
        let r in 0 .. 1;
        let mut res = None;
        let exec = Benches::<T>::termination_bench(SyscallName::WaitUpTo, Some(100), r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_wake {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_wake(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_create_program {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_create_program(r, None, None, false)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_create_program_per_kb {
        let p in 0 .. MAX_PAYLOAD_LEN_KB;
        // salt cannot be zero because we cannot execute batch of syscalls
        // as salt will be the same and we will get `ProgramAlreadyExists` error
        let s in 1 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_create_program(1, Some(p), Some(s), false)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_create_program_wgas {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_create_program(r, None, None, true)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_create_program_wgas_per_kb {
        let p in 0 .. MAX_PAYLOAD_LEN_KB;
        // salt cannot be zero because we cannot execute batch of syscalls
        // as salt will be the same and we will get `ProgramAlreadyExists` error
        let s in 1 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_create_program(1, Some(p), Some(s), true)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    lazy_pages_signal_read {
        let p in 0 .. DEFAULT_MEM_SIZE as u32;
        let mut res = None;
        let exec = Benches::<T>::lazy_pages_signal_read((p as u16).into())?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    lazy_pages_signal_write {
        let p in 0 .. DEFAULT_MEM_SIZE as u32;
        let mut res = None;
        let exec = Benches::<T>::lazy_pages_signal_write((p as u16).into())?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    lazy_pages_signal_write_after_read {
        let p in 0 .. DEFAULT_MEM_SIZE as u32;
        let mut res = None;
        let exec = Benches::<T>::lazy_pages_signal_write_after_read((p as u16).into(), DEFAULT_MEM_SIZE.into())?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    lazy_pages_load_page_storage_data {
        let p in 0 .. DEFAULT_MEM_SIZE as u32;
        let mut res = None;
        let exec = Benches::<T>::lazy_pages_load_page_storage_data((p as u16).into())?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    lazy_pages_host_func_read {
        let p in 0 .. MAX_PAYLOAD_LEN / WasmPage::SIZE;
        let mut res = None;
        let exec = Benches::<T>::lazy_pages_host_func_read((p as u16).into())?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    lazy_pages_host_func_write {
        let p in 0 .. MAX_PAYLOAD_LEN / WasmPage::SIZE;
        let mut res = None;
        let exec = Benches::<T>::lazy_pages_host_func_write((p as u16).into())?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    lazy_pages_host_func_write_after_read {
        let p in 0 .. MAX_PAYLOAD_LEN / WasmPage::SIZE;
        let mut res = None;
        let exec = Benches::<T>::lazy_pages_host_func_write_after_read((p as u16).into())?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // w_load = w_bench
    instr_i64load {
        // Increased interval in order to increase accuracy
        let r in INSTR_BENCHMARK_BATCHES .. 10 * INSTR_BENCHMARK_BATCHES;
        let mem_pages = DEFAULT_MEM_SIZE;
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(mem_pages)),
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                        RandomUnaligned(0, mem_pages as u32 * WasmPage::SIZE - 8),
                        Regular(Instruction::I64Load(MemArg::i64())),
                        Regular(Instruction::Drop)])),
            .. Default::default()
        };
        let mut sbox = Sandbox::from_module_def::<T>(module);
    }: {
        sbox.invoke();
    }

    // w_load = w_bench
    instr_i32load {
        // Increased interval in order to increase accuracy
        let r in INSTR_BENCHMARK_BATCHES .. 10 * INSTR_BENCHMARK_BATCHES;
        let mem_pages = DEFAULT_MEM_SIZE;
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(mem_pages)),
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                        RandomUnaligned(0, mem_pages as u32 * WasmPage::SIZE - 4),
                        Regular(Instruction::I32Load(MemArg::i32())),
                        Regular(Instruction::Drop)])),
            .. Default::default()
        };
        let mut sbox = Sandbox::from_module_def::<T>(module);
    }: {
        sbox.invoke();
    }

    // w_store = w_bench - w_i64const
    instr_i64store {
        // Increased interval in order to increase accuracy
        let r in INSTR_BENCHMARK_BATCHES .. 10 * INSTR_BENCHMARK_BATCHES;
        let mem_pages = DEFAULT_MEM_SIZE;
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(mem_pages)),
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                        RandomUnaligned(0, mem_pages as u32 * WasmPage::SIZE - 8),
                        RandomI64Repeated(1),
                        Regular(Instruction::I64Store(MemArg::i64()))])),
            .. Default::default()
        };
        let mut sbox = Sandbox::from_module_def::<T>(module);
    }: {
        sbox.invoke();
    }

    // w_store = w_bench
    instr_i32store {
        // Increased interval in order to increase accuracy
        let r in INSTR_BENCHMARK_BATCHES .. 10 * INSTR_BENCHMARK_BATCHES;
        let mem_pages = DEFAULT_MEM_SIZE;
        let module = ModuleDefinition {
            memory: Some(ImportedMemory::new(mem_pages)),
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                        RandomUnaligned(0, mem_pages as u32 * WasmPage::SIZE - 4),
                        RandomI32Repeated(1),
                        Regular(Instruction::I32Store(MemArg::i32()))])),
            .. Default::default()
        };
        let mut sbox = Sandbox::from_module_def::<T>(module);
    }: {
        sbox.invoke();
    }

    // w_select = w_bench - 2 * w_i64const
    instr_select {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
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

    // w_if = w_bench
    instr_if {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut instructions = body::repeated_dyn_instr(
            r * INSTR_BENCHMARK_BATCH_SIZE,
            vec![
                Regular(Instruction::If(BlockType::Type(ValType::I32) )),
                RandomI32Repeated(1),
                Regular(Instruction::Else),
                RandomI32Repeated(1),
                Regular(Instruction::End),
            ],
            vec![Instruction::I32Const(1 )],
        );
        instructions.push(Instruction::Drop);
        let body = body::from_instructions(instructions);
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body),
            ..Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_br = w_bench
    instr_br {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                Regular(Instruction::Block(BlockType::Empty)),
                Regular(Instruction::Br(0)),
                Regular(Instruction::End),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_br_if = w_bench - w_i64const
    instr_br_if {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                Regular(Instruction::Block(BlockType::Empty)),
                RandomI32(0, 2),
                Regular(Instruction::BrIf(0 )),
                Regular(Instruction::End),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_br_table = w_bench
    instr_br_table {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                Regular(Instruction::Block(BlockType::Empty)),
                RandomI32Repeated(1),
                Regular(Instruction::BrTable(BrTable { default: 0, targets: vec![0] } )),
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
        let targets: Vec<u32> = [0, 1].iter()
            .cloned()
            .cycle()
            .take((e / 2) as usize).collect();
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(INSTR_BENCHMARK_BATCH_SIZE, vec![
                Regular(Instruction::Block(BlockType::Empty)),
                Regular(Instruction::Block(BlockType::Empty)),
                Regular(Instruction::Block(BlockType::Empty)),
                RandomI32(0, (e + 1) as i32), // Make sure the default entry is also used
                Regular(Instruction::BrTable(BrTable { default: 0, targets } )),
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

     // w_i64const = w_bench - w_call
     instr_call_const {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            aux_body: Some(body::from_instructions(vec![Instruction::I64Const(0x7ffffffff3ffffff )])),
            aux_res: Some(ValType::I64),
            handle_body: Some(body::repeated(r * INSTR_BENCHMARK_BATCH_SIZE, &[
                Instruction::Call(OFFSET_AUX ),
                Instruction::Drop,
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_call = w_bench
    instr_call {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            aux_body: Some(body::empty()),
            handle_body: Some(body::repeated(r * INSTR_BENCHMARK_BATCH_SIZE, &[
                Instruction::Call(OFFSET_AUX ),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_call_indirect = w_bench
    instr_call_indirect {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let num_elements = T::Schedule::get().limits.table_size;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            aux_body: Some(body::empty()),
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI32(0, num_elements as i32),
                Regular(Instruction::CallIndirect(0)),
            ])),
            table: Some(TableSegment {
                num_elements,
                function_index: OFFSET_AUX,
                init_elements: Default::default(),
            }),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_instr_call_indirect_per_param = w_bench - w_i64const
    // Calling a function indirectly causes it to go through a thunk function whose runtime
    // linearly depend on the amount of parameters to this function.
    instr_call_indirect_per_param {
        let p in 0 .. T::Schedule::get().limits.parameters;
        let num_elements = T::Schedule::get().limits.table_size;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            aux_body: Some(body::empty()),
            aux_arg_num: p,
            handle_body: Some(body::repeated_dyn(INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI64Repeated(p as usize),
                RandomI32(0, num_elements as i32),
                Regular(Instruction::CallIndirect(p.min(1))), // aux signature: 1 or 0
            ])),
            table: Some(TableSegment {
                num_elements,
                function_index: OFFSET_AUX,
                init_elements: Default::default(),
            }),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_per_local = w_bench
    instr_call_per_local {
        let l in 0 .. T::Schedule::get().limits.locals;
        let mut aux_body = body::empty();
        body::inject_locals(&mut aux_body, l);
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            aux_body: Some(aux_body),
            handle_body: Some(body::repeated(INSTR_BENCHMARK_BATCH_SIZE, &[
                Instruction::Call(2 ), // call aux
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_local_get = w_bench
    instr_local_get {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let max_locals = T::Schedule::get().limits.stack_height.unwrap_or(512);
        let mut handle_body = body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
            RandomGetLocal(0, max_locals),
            Regular(Instruction::Drop),
        ]);
        body::inject_locals(&mut handle_body, max_locals);
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(handle_body),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_local_set = w_bench - w_i64const
    instr_local_set {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let max_locals = T::Schedule::get().limits.stack_height.unwrap_or(512);
        let mut handle_body = body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
            RandomI64Repeated(1),
            RandomSetLocal(0, max_locals),
        ]);
        body::inject_locals(&mut handle_body, max_locals);
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(handle_body),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_local_tee = w_bench - w_i64const
    instr_local_tee {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let max_locals = T::Schedule::get().limits.stack_height.unwrap_or(512);
        let mut handle_body = body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
            RandomI64Repeated(1),
            RandomTeeLocal(0, max_locals),
            Regular(Instruction::Drop),
        ]);
        body::inject_locals(&mut handle_body, max_locals);
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(handle_body),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_global_get = w_bench
    instr_global_get {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let max_globals = T::Schedule::get().limits.globals;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomGetGlobal(0, max_globals),
                Regular(Instruction::Drop),
            ])),
            num_globals: max_globals,
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_global_set = w_bench - w_i64const
    instr_global_set {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let max_globals = T::Schedule::get().limits.globals;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI64Repeated(1),
                RandomSetGlobal(0, max_globals),
            ])),
            num_globals: max_globals,
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_memory_get = w_bench
    instr_memory_current {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::repeated(r * INSTR_BENCHMARK_BATCH_SIZE, &[
                Instruction::MemorySize(0),
                Instruction::Drop
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // Unary numeric instructions.
    // All use w = w_bench - w_i64const

    instr_i64clz {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr_64(
            Instruction::I64Clz,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32clz {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr_32(
            Instruction::I32Clz,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64ctz {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr_64(
            Instruction::I64Ctz,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32ctz {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr_32(
            Instruction::I32Ctz,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64popcnt {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr_64(
            Instruction::I64Popcnt,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32popcnt {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr_32(
            Instruction::I32Popcnt,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64eqz {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr_64(
            Instruction::I64Eqz,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32eqz {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr_32(
            Instruction::I32Eqz,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    // w_extend = w_bench
    //
    // i32.extend8_s
    instr_i32extend8s {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI32Repeated(1),
                Regular(Instruction::I32Extend8S),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_extend = w_bench
    //
    // i32.extend16_s
    instr_i32extend16s {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI32Repeated(1),
                Regular(Instruction::I32Extend16S),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_extend = w_bench
    //
    // i64.extend8_s
    instr_i64extend8s {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI64Repeated(1),
                Regular(Instruction::I64Extend8S),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_extend = w_bench
    //
    // i64.extend16_s
    instr_i64extend16s {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI64Repeated(1),
                Regular(Instruction::I64Extend16S),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_extend = w_bench
    //
    // i64.extend32_s
    instr_i64extend32s {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI64Repeated(1),
                Regular(Instruction::I64Extend32S),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_extends = w_bench
    instr_i64extendsi32 {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI32Repeated(1),
                Regular(Instruction::I64ExtendI32S),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_extendu = w_bench
    instr_i64extendui32 {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI32Repeated(1),
                Regular(Instruction::I64ExtendI32U),
                Regular(Instruction::Drop),
            ])),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    instr_i32wrapi64 {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::unary_instr_64(
            Instruction::I32WrapI64,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    // Binary numeric instructions.
    // All use w = w_bench - 2 * w_i64const

    instr_i64eq {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64Eq,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32eq {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32Eq,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64ne {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64Ne,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32ne {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32Ne,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64lts {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64LtS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32lts {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32LtS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64ltu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64LtU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32ltu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32LtU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64gts {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64GtS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32gts {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32GtS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64gtu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64GtU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32gtu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32GtU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64les {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64LeS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32les {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32LeS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64leu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64LeU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32leu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32LeU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64ges {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64GeS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32ges {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32GeS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64geu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64GeU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32geu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32GeU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64add {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64Add,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32add {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32Add,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64sub {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64Sub,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32sub {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32Sub,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64mul {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64Mul,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32mul {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32Mul,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64divs {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64DivS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32divs {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32DivS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64divu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64DivU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32divu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32DivU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64rems {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64RemS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32rems {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32RemS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64remu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64RemU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32remu {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32RemU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64and {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64And,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32and {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32And,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64or {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64Or,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32or {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32Or,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64xor {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64Xor,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32xor {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32Xor,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64shl {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64Shl,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32shl {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32Shl,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64shrs {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64ShrS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32shrs {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32ShrS,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64shru {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64ShrU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32shru {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32ShrU,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64rotl {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64Rotl,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32rotl {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32Rotl,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i64rotr {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_64(
            Instruction::I64Rotr,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    instr_i32rotr {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::binary_instr_32(
            Instruction::I32Rotr,
            r * INSTR_BENCHMARK_BATCH_SIZE,
        ));
    }: {
        sbox.invoke();
    }

    tasks_remove_gas_reservation {
        let (program_id, reservation_id) = tasks::remove_gas_reservation::<T>();
        let (builtins, _) = T::BuiltinDispatcherFactory::create();
        let mut ext_manager = ExtManager::<T>::new(builtins);
    }: {
        ext_manager.remove_gas_reservation(program_id, reservation_id);
    }

    tasks_send_user_message_to_mailbox {
        let message_id = tasks::send_user_message::<T>();
        let (builtins, _) = T::BuiltinDispatcherFactory::create();
        let mut ext_manager = ExtManager::<T>::new(builtins);
    }: {
        ext_manager.send_user_message(message_id, true);
    }

    tasks_send_user_message {
        let message_id = tasks::send_user_message::<T>();
        let (builtins, _) = T::BuiltinDispatcherFactory::create();
        let mut ext_manager = ExtManager::<T>::new(builtins);
    }: {
        ext_manager.send_user_message(message_id, false);
    }

    tasks_send_dispatch {
        let message_id = tasks::send_dispatch::<T>();

        let (builtins, _) = T::BuiltinDispatcherFactory::create();
        let mut ext_manager = ExtManager::<T>::new(builtins);
    }: {
        ext_manager.send_dispatch(message_id);
    }

    tasks_wake_message {
        let (program_id, message_id) = tasks::wake_message::<T>();

        let (builtins, _) = T::BuiltinDispatcherFactory::create();
        let mut ext_manager = ExtManager::<T>::new(builtins);
    }: {
        ext_manager.wake_message(program_id, message_id);
    }

    tasks_wake_message_no_wake {
        let (builtins, _) = T::BuiltinDispatcherFactory::create();
        let mut ext_manager = ExtManager::<T>::new(builtins);
    }: {
        ext_manager.wake_message(Default::default(), Default::default());
    }

    tasks_remove_from_waitlist {
        let (program_id, message_id) = tasks::remove_from_waitlist::<T>();

        let (builtins, _) = T::BuiltinDispatcherFactory::create();
        let mut ext_manager = ExtManager::<T>::new(builtins);
    }: {
        ext_manager.remove_from_waitlist(program_id, message_id);
    }

    tasks_remove_from_mailbox {
        let (user, message_id) = tasks::remove_from_mailbox::<T>();

        let (builtins, _) = T::BuiltinDispatcherFactory::create();
        let mut ext_manager = ExtManager::<T>::new(builtins);
    }: {
        ext_manager.remove_from_mailbox(user.cast(), message_id);
    }

    // This is no benchmark. It merely exist to have an easy way to pretty print the currently
    // configured `Schedule` during benchmark development.
    // It can be outputted using the following command:
    // cargo run --release --features runtime-benchmarks \
    //     -- benchmark --extra --dev --execution=native \
    //     -p pallet_gear -e print_schedule --no-median-slopes --no-min-squares
    #[extra]
    print_schedule {
        #[cfg(feature = "std")]
        {
            println!("{:#?}", Schedule::<T>::default());
        }
        #[cfg(not(feature = "std"))]
        Err("Run this bench with a native runtime in order to see the schedule.")?;
    }: {}

    impl_benchmark_test_suite!(
        Gear, crate::mock::new_test_ext(), crate::mock::Test
    )
}
