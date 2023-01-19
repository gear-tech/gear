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

#![cfg(feature = "runtime-benchmarks")]

#[allow(dead_code)]
mod code;
mod sandbox;

mod syscalls;
use syscalls::Benches;

mod tests;
use tests::syscalls_integrity;

use self::{
    code::{
        body::{self, DynInstr::*},
        max_pages, ImportedMemory, Location, ModuleDefinition, TableSegment, WasmModule,
        OFFSET_AUX,
    },
    sandbox::Sandbox,
};
use crate::{
    manager::ExtManager, pallet, schedule::INSTR_BENCHMARK_BATCH_SIZE, BTreeMap, BalanceOf,
    BenchmarkStorage, Call, Config, ExecutionEnvironment, Ext as Externalities, GasHandlerOf,
    MailboxOf, Pallet as Gear, Pallet, ProgramStorageOf, QueueOf, Schedule,
};
use ::alloc::vec;
use codec::Encode;
use common::{
    self, benchmarking,
    storage::{Counter, *},
    CodeMetadata, CodeStorage, GasPrice, GasTree, Origin,
};
use core::ops::Range;
use core_processor::{
    common::{DispatchOutcome, JournalNote},
    configs::BlockConfig,
    ProcessExecutionContext, ProcessorContext, ProcessorExt,
};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::{Currency, Get, Hooks, ReservableCurrency};
use frame_system::{Pallet as SystemPallet, RawOrigin};
use gear_backend_common::Environment;
use gear_core::{
    code::{Code, CodeAndId},
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    ids::{MessageId, ProgramId},
    memory::{AllocationsContext, PageBuf, PageNumber, PageU32Size, WasmPageNumber},
    message::{ContextSettings, DispatchKind, MessageContext},
    reservation::GasReserver,
};
use gear_wasm_instrument::{
    parity_wasm::elements::{BlockType, BrTableData, Instruction, ValueType},
    syscalls::SysCallName,
};
use pallet_authorship::Pallet as AuthorshipPallet;
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    Slot, BABE_ENGINE_ID,
};
use sp_core::H256;
use sp_runtime::{
    traits::{Bounded, CheckedAdd, One, UniqueSaturatedInto, Zero},
    Digest, DigestItem, Perbill,
};
use sp_std::prelude::*;

const MAX_PAYLOAD_LEN: u32 = 16 * 64 * 1024;
const MAX_PAYLOAD_LEN_KB: u32 = MAX_PAYLOAD_LEN / 1024;
const MAX_PAGES: u32 = 512;

/// How many batches we do per API benchmark.
const API_BENCHMARK_BATCHES: u32 = 20;

/// How many batches we do per Instruction benchmark.
const INSTR_BENCHMARK_BATCHES: u32 = 50;

// Initializes new block.
fn init_block<T: Config>(previous: Option<T::BlockNumber>)
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

    Gear::<T>::process_queue(Default::default());
}

fn default_processor_context<T: Config>() -> ProcessorContext {
    ProcessorContext {
        gas_counter: GasCounter::new(0),
        gas_allowance_counter: GasAllowanceCounter::new(0),
        gas_reserver: GasReserver::new(
            Default::default(),
            0,
            Default::default(),
            T::ReservationsLimit::get(),
        ),
        system_reservation: None,
        value_counter: ValueCounter::new(0),
        allocations_context: AllocationsContext::new(
            Default::default(),
            Default::default(),
            Default::default(),
        ),
        message_context: MessageContext::new(
            Default::default(),
            Default::default(),
            None,
            ContextSettings::new(0, 0, 0, 0, 0, 0),
        ),
        block_info: Default::default(),
        config: Default::default(),
        existential_deposit: 0,
        origin: Default::default(),
        program_id: Default::default(),
        program_candidates_data: Default::default(),
        host_fn_weights: Default::default(),
        forbidden_funcs: Default::default(),
        mailbox_threshold: 0,
        waitlist_cost: 0,
        reserve_for: 0,
        reservation: 0,
        random_data: ([0u8; 32].to_vec(), 0),
    }
}

fn verify_process((notes, err_len_ptrs): (Vec<JournalNote>, Range<u32>)) {
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

    let start_page_number = PageNumber::from_offset(err_len_ptrs.start);
    let end_page_number = PageNumber::from_offset(err_len_ptrs.end);
    start_page_number
        .iter_end_inclusive(end_page_number)
        .unwrap()
        .flat_map(|page_number| {
            pages_data
                .get(&page_number)
                .map(|page| (page as &[u8]).to_vec())
                .unwrap_or_else(|| vec![0; PageNumber::size() as usize])
        })
        .skip(err_len_ptrs.start as usize)
        .take(err_len_ptrs.len())
        .all(|b| b == 0)
        .then_some(())
        .expect("Fallible sys-call returned error")
}

fn run_process<T>(exec: Exec<T>) -> (Vec<JournalNote>, Range<u32>)
where
    T: Config,
    T::AccountId: Origin,
{
    (
        core_processor::process::<Externalities, ExecutionEnvironment>(
            &exec.block_config,
            exec.context,
            exec.random_data,
            exec.memory_pages,
        ),
        exec.err_len_ptrs,
    )
}

/// An instantiated and deployed program.
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
        let value = <T as pallet::Config>::Currency::minimum_balance();
        <T as pallet::Config>::Currency::make_free_balance_be(&caller, caller_funding::<T>());
        let salt = vec![0xff];
        let addr = ProgramId::generate(module.hash, &salt).into_origin();

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
    memory_pages: BTreeMap<PageNumber, PageBuf>,
    err_len_ptrs: Range<u32>,
}

benchmarks! {

    where_clause { where
        T::AccountId: Origin,
    }

    #[extra]
    check_syscalls_integrity {
        syscalls_integrity::main_test::<T>();
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

    // `c`: Size of the code in kilobytes.
    instantiate_module_per_kb {
        let c in 0 .. T::Schedule::get().limits.code_len / 1024;

        let WasmModule { code, .. } = WasmModule::<T>::sized(c * 1024, Location::Init);
    }: {
        let ext = Externalities::new(default_processor_context::<T>());
        ExecutionEnvironment::new(ext, &code, DispatchKind::Init, Default::default(), max_pages::<T>().into()).unwrap();
    }

    claim_value {
        let caller = benchmarking::account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let program_id = benchmarking::account::<T::AccountId>("program", 0, 100);
        <T as pallet::Config>::Currency::deposit_creating(&program_id, 100_000_000_000_000_u128.unique_saturated_into());
        let code = benchmarking::generate_wasm2(16.into()).unwrap();
        benchmarking::set_program::<ProgramStorageOf::<T>>(ProgramId::from_origin(program_id.clone().into_origin()), code, 1.into());
        let original_message_id = MessageId::from_origin(benchmarking::account::<T::AccountId>("message", 0, 100).into_origin());
        let gas_limit = 50000;
        let value = 10000u32.into();
        GasHandlerOf::<T>::create(program_id.clone(), original_message_id, gas_limit).expect("Failed to create gas handler");
        <T as pallet::Config>::Currency::reserve(&program_id, <T as pallet::Config>::GasPrice::gas_price(gas_limit) + value).expect("Failed to reserve");
        MailboxOf::<T>::insert(gear_core::message::StoredMessage::new(
            original_message_id,
            ProgramId::from_origin(program_id.into_origin()),
            ProgramId::from_origin(caller.clone().into_origin()),
            Default::default(),
            value.unique_saturated_into(),
            None,
        ), u32::MAX.unique_saturated_into()).expect("Error during mailbox insertion");

        init_block::<T>(None);
    }: _(RawOrigin::Signed(caller.clone()), original_message_id)
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(None)));
        assert!(MailboxOf::<T>::is_empty(&caller));
    }

    // This constructs a program that is maximal expensive to instrument.
    // It creates a maximum number of metering blocks per byte.
    //
    // `c`: Size of the code in kilobytes.
    upload_code {
        let c in 0 .. Perbill::from_percent(49).mul_ceil(T::Schedule::get().limits.code_len) / 1024;
        let value = <T as pallet::Config>::Currency::minimum_balance();
        let caller = whitelisted_caller();
        <T as pallet::Config>::Currency::make_free_balance_be(&caller, caller_funding::<T>());
        let WasmModule { code, hash: code_id, .. } = WasmModule::<T>::sized(c * 1024, Location::Handle);
        let origin = RawOrigin::Signed(caller);

        init_block::<T>(None);
    }: _(origin, code)
    verify {
        assert!(<T as pallet::Config>::CodeStorage::exists(code_id));
    }

    // The size of the salt influences the runtime because is is hashed in order to
    // determine the program address.
    //
    // `s`: Size of the salt in kilobytes.
    create_program {
        let s in 0 .. code::max_pages::<T>() as u32 * 64 * 128;

        let caller = whitelisted_caller();
        let origin = RawOrigin::Signed(caller);

        let WasmModule { code, hash: code_id, .. } = WasmModule::<T>::dummy();
        Gear::<T>::upload_code(origin.into(), code).expect("submit code failed");

        let salt = vec![42u8; s as usize];
        let value = <T as pallet::Config>::Currency::minimum_balance();
        let caller = whitelisted_caller();
        <T as pallet::Config>::Currency::make_free_balance_be(&caller, caller_funding::<T>());
        let origin = RawOrigin::Signed(caller);

        init_block::<T>(None);
    }: _(origin, code_id, salt, vec![], 100_000_000_u64, value)
    verify {
        assert!(<T as pallet::Config>::CodeStorage::exists(code_id));
    }

    // This constructs a program that is maximal expensive to instrument.
    // It creates a maximum number of metering blocks per byte.
    // The size of the salt influences the runtime because is is hashed in order to
    // determine the program address.
    //
    // `c`: Size of the code in kilobytes.
    // `s`: Size of the salt in kilobytes.
    //
    // # Note
    //
    // We cannot let `c` grow to the maximum code size because the code is not allowed
    // to be larger than the maximum size **after instrumentation**.
    upload_program {
        let c in 0 .. Perbill::from_percent(49).mul_ceil(T::Schedule::get().limits.code_len) / 1024;
        let s in 0 .. code::max_pages::<T>() as u32 * 64 * 128;
        let salt = vec![42u8; s as usize];
        let value = <T as pallet::Config>::Currency::minimum_balance();
        let caller = whitelisted_caller();
        <T as pallet::Config>::Currency::make_free_balance_be(&caller, caller_funding::<T>());
        let WasmModule { code, hash, .. } = WasmModule::<T>::sized(c * 1024, Location::Handle);
        let origin = RawOrigin::Signed(caller);

        init_block::<T>(None);
    }: _(origin, code, salt, vec![], 100_000_000_u64, value)
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(Some(_))));
    }

    send_message {
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller = benchmarking::account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let minimum_balance = <T as pallet::Config>::Currency::minimum_balance();
        let program_id = ProgramId::from_origin(benchmarking::account::<T::AccountId>("program", 0, 100).into_origin());
        let code = benchmarking::generate_wasm2(16.into()).unwrap();
        benchmarking::set_program::<ProgramStorageOf::<T>>(program_id, code, 1.into());
        let payload = vec![0_u8; p as usize];

        init_block::<T>(None);
    }: _(RawOrigin::Signed(caller), program_id, payload, 100_000_000_u64, minimum_balance)
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(Some(_))));
    }

    send_reply {
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller = benchmarking::account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let minimum_balance = <T as pallet::Config>::Currency::minimum_balance();
        let program_id = benchmarking::account::<T::AccountId>("program", 0, 100);
        <T as pallet::Config>::Currency::deposit_creating(&program_id, 100_000_000_000_000_u128.unique_saturated_into());
        let code = benchmarking::generate_wasm2(16.into()).unwrap();
        benchmarking::set_program::<ProgramStorageOf::<T>>(ProgramId::from_origin(program_id.clone().into_origin()), code, 1.into());
        let original_message_id = MessageId::from_origin(benchmarking::account::<T::AccountId>("message", 0, 100).into_origin());
        let gas_limit = 50000;
        let value = (p % 2).into();
        GasHandlerOf::<T>::create(program_id.clone(), original_message_id, gas_limit).expect("Failed to create gas handler");
        <T as pallet::Config>::Currency::reserve(&program_id, <T as pallet::Config>::GasPrice::gas_price(gas_limit) + value).expect("Failed to reserve");
        MailboxOf::<T>::insert(gear_core::message::StoredMessage::new(
            original_message_id,
            ProgramId::from_origin(program_id.into_origin()),
            ProgramId::from_origin(caller.clone().into_origin()),
            Default::default(),
            value.unique_saturated_into(),
            None,
        ), u32::MAX.unique_saturated_into()).expect("Error during mailbox insertion");
        let payload = vec![0_u8; p as usize];

        init_block::<T>(None);
    }: _(RawOrigin::Signed(caller.clone()), original_message_id, payload, 100_000_000_u64, minimum_balance)
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(Some(_))));
        assert!(MailboxOf::<T>::is_empty(&caller))
    }

    initial_allocation {
        let q in 1 .. MAX_PAGES;
        let q = q as u16;
        let caller: T::AccountId = benchmarking::account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, (1u128 << 60).unique_saturated_into());
        let code = benchmarking::generate_wasm(q.into()).unwrap();
        let salt = vec![255u8; 32];
    }: {
        let _ = Gear::<T>::upload_program(RawOrigin::Signed(caller).into(), code, salt, vec![], 100_000_000u64, 0u32.into());
        process_queue::<T>();
    }
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(None)));
    }

    alloc_in_handle {
        let q in 0 .. MAX_PAGES;
        let q = q as u16;
        let caller: T::AccountId = benchmarking::account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, (1_u128 << 60).unique_saturated_into());
        let code = benchmarking::generate_wasm2(q.into()).unwrap();
        let salt = vec![255u8; 32];
    }: {
        let _ = Gear::<T>::upload_program(RawOrigin::Signed(caller).into(), code, salt, vec![], 100_000_000u64, 0u32.into());
        process_queue::<T>();
    }
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(None)));
    }

    // This benchmarks the additional weight that is charged when a program is executed the
    // first time after a new schedule was deployed: For every new schedule a program needs
    // to re-run the instrumentation once.
    reinstrument_per_kb {
        let c in 0 .. T::Schedule::get().limits.code_len / 1_024;
        let WasmModule { code, hash, .. } = WasmModule::<T>::sized(c * 1_024, Location::Handle);
        let code = Code::new_raw(code, 1, None, false, true).unwrap();
        let code_and_id = CodeAndId::new(code);
        let code_id = code_and_id.code_id();

        let caller: T::AccountId = benchmarking::account("caller", 0, 0);
        let metadata = {
            let block_number = Pallet::<T>::block_number().unique_saturated_into();
            CodeMetadata::new(caller.into_origin(), block_number)
        };

        T::CodeStorage::add_code(code_and_id, metadata).unwrap();

        let schedule = T::Schedule::get();
    }: {
        Gear::<T>::reinstrument_code(code_id, &schedule);
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

    gr_reserve_gas {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_reserve_gas(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_unreserve_gas {
        let r in 0 .. API_BENCHMARK_BATCHES;
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
        let exec = Benches::<T>::getter(SysCallName::MessageId, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_origin {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SysCallName::Origin, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_program_id {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SysCallName::ProgramId, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_source {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SysCallName::Source, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_value {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SysCallName::Value, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_value_available {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SysCallName::ValueAvailable, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_gas_available {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SysCallName::GasAvailable, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_size {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SysCallName::Size, r)?;
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

    gr_block_height {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SysCallName::BlockHeight, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_block_timestamp {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::getter(SysCallName::BlockTimestamp, r)?;
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
        let exec = Benches::<T>::gr_send_commit(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_commit_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_send_commit_per_kb(n)?;
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

    gr_reservation_send_commit_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_reservation_send_commit_per_kb(n)?;
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
        let exec = Benches::<T>::gr_reply_commit(r)?;
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
        let n in 0 .. gear_core::message::MAX_PAYLOAD_SIZE as u32 / 1024;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_push_per_kb(n)?;
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
        let exec = Benches::<T>::gr_reply_push_input(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reply_push_input_per_kb {
        let n in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let mut res = None;
        let exec = Benches::<T>::gr_reply_push_input_per_kb(n)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_push_input {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_send_push_input(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_push_input_per_kb {
        let n in 0 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_send_push_input_per_kb(n)?;
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

    gr_error {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_error(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_status_code {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_status_code(r)?;
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
        let exec = Benches::<T>::termination_bench(SysCallName::Exit, Some(0xff), r)?;
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
        let exec = Benches::<T>::termination_bench(SysCallName::Leave, None, r)?;
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
        let exec = Benches::<T>::termination_bench(SysCallName::Wait, None, r)?;
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
        let exec = Benches::<T>::termination_bench(SysCallName::WaitFor, Some(10), r)?;
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
        let exec = Benches::<T>::termination_bench(SysCallName::WaitUpTo, Some(100), r)?;
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

    gr_create_program_wgas {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = Benches::<T>::gr_create_program_wgas(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_create_program_wgas_per_kb {
        let p in 0 .. MAX_PAYLOAD_LEN_KB;
        // salt cannot be zero because we cannot execute batch of sys-calls
        // as salt will be the same and we will get `ProgramAlreadyExists` error
        let s in 1 .. MAX_PAYLOAD_LEN_KB;
        let mut res = None;
        let exec = Benches::<T>::gr_create_program_wgas_per_kb(p, s)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    lazy_pages_read_access {
        let p in 0 .. code::max_pages::<T>() as u32;
        let mut res = None;
        let exec = Benches::<T>::lazy_pages_read_access((p as u16).into())?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    lazy_pages_write_access {
        let p in 0 .. code::max_pages::<T>() as u32;
        let mut res = None;
        let exec = Benches::<T>::lazy_pages_write_access((p as u16).into())?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // w_load = w_bench
    instr_i64load {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mem_pages = code::max_pages::<T>() as u32;
        // Warm up memory.
        let mut instrs = body::write_access_all_pages_instrs((mem_pages as u16).into(), vec![]);
        instrs = body::repeated_dyn_instr(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                        RandomUnaligned(0, mem_pages * WasmPageNumber::size() - 8),
                        Regular(Instruction::I64Load(3, 0)),
                        Regular(Instruction::Drop)], instrs);
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory{min_pages: mem_pages}),
            handle_body: Some(body::from_instructions(instrs)),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_store = w_bench - w_i64const
    instr_i64store {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mem_pages = code::max_pages::<T>() as u32;
        // Warm up memory.
        let mut instrs = body::write_access_all_pages_instrs((mem_pages as u16).into(), vec![]);
        instrs = body::repeated_dyn_instr(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                        RandomUnaligned(0, mem_pages * WasmPageNumber::size() - 8),
                        RandomI64Repeated(1),
                        Regular(Instruction::I64Store(3, 0))], instrs);
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory{min_pages: mem_pages}),
            handle_body: Some(body::from_instructions(instrs)),
            .. Default::default()
        }));
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
                Regular(Instruction::If(BlockType::Value(ValueType::I32))),
                RandomI32Repeated(1),
                Regular(Instruction::Else),
                RandomI32Repeated(1),
                Regular(Instruction::End),
            ],
            vec![Instruction::I32Const(1)],
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
                Regular(Instruction::Block(BlockType::NoResult)),
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
                Regular(Instruction::Block(BlockType::NoResult)),
                RandomI32(0, 2),
                Regular(Instruction::BrIf(0)),
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
        let table = Box::new(BrTableData {
            table: Box::new([0]),
            default: 0,
        });
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                Regular(Instruction::Block(BlockType::NoResult)),
                RandomI32Repeated(1),
                Regular(Instruction::BrTable(table)),
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
            handle_body: Some(body::repeated_dyn(INSTR_BENCHMARK_BATCH_SIZE, vec![
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

    // w_call = w_bench
    instr_call {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            aux_body: Some(body::plain(vec![Instruction::End])),
            handle_body: Some(body::repeated(r * INSTR_BENCHMARK_BATCH_SIZE, &[
                Instruction::Call(OFFSET_AUX),
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
             aux_body: Some(body::plain(vec![Instruction::I64Const(0x7ffffffff3ffffff), Instruction::End])),
             aux_res: Some(ValueType::I64),
             handle_body: Some(body::repeated(r * INSTR_BENCHMARK_BATCH_SIZE, &[
                 Instruction::Call(OFFSET_AUX),
                 Instruction::Drop,
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
            aux_body: Some(body::plain(vec![Instruction::End])),
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI32(0, num_elements as i32),
                Regular(Instruction::CallIndirect(0, 0)),
            ])),
            table: Some(TableSegment {
                num_elements,
                function_index: OFFSET_AUX,
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
            aux_body: Some(body::plain(vec![Instruction::End])),
            aux_arg_num: p,
            handle_body: Some(body::repeated_dyn(INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI64Repeated(p as usize),
                RandomI32(0, num_elements as i32),
                Regular(Instruction::CallIndirect(p.min(1), 0)), // aux signature: 1 or 0
            ])),
            table: Some(TableSegment {
                num_elements,
                function_index: OFFSET_AUX,
            }),
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
                Instruction::CurrentMemory(0),
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

    // w_extends = w_bench
    instr_i64extendsi32 {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI32Repeated(1),
                Regular(Instruction::I64ExtendSI32),
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
    // All use w = w_bench - 2 * w_i64const

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
