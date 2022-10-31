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

#![cfg(feature = "runtime-benchmarks")]

#[allow(dead_code)]
mod code;
mod sandbox;
mod syscalls;
use syscalls::*;

use self::{
    code::{
        body::{self, DynInstr::*},
        max_pages, ImportedMemory, Location, ModuleDefinition, WasmModule, OFFSET_AUX,
    },
    sandbox::Sandbox,
};
use crate::{
    manager::ExtManager, pallet, schedule::INSTR_BENCHMARK_BATCH_SIZE, BTreeMap, BalanceOf,
    BenchmarkStorage, Call, Config, ExecutionEnvironment, Ext as Externalities, GasHandlerOf,
    MailboxOf, Pallet as Gear, Pallet, QueueOf, Schedule,
};
use codec::Encode;
use common::{benchmarking, storage::*, CodeMetadata, CodeStorage, GasPrice, GasTree, Origin};
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
    memory::{AllocationsContext, PageBuf, PageNumber},
    message::{ContextSettings, MessageContext},
    reservation::GasReserver,
};
use gear_wasm_instrument::parity_wasm::elements::{BlockType, BrTableData, Instruction, ValueType};
use pallet_authorship::Pallet as AuthorshipPallet;
use sp_consensus_babe::{
    digests::{PreDigest, SecondaryPlainPreDigest},
    Slot, BABE_ENGINE_ID,
};
use sp_core::H256;
use sp_runtime::{
    traits::{Bounded, One, UniqueSaturatedInto},
    Digest, DigestItem, Perbill,
};
use sp_std::prelude::*;

const MAX_PAYLOAD_LEN: u32 = 16 * 64 * 1024;
const MAX_PAGES: u32 = 512;

/// How many batches we do per API benchmark.
const API_BENCHMARK_BATCHES: u32 = 20;

/// How many batches we do per Instruction benchmark.
const INSTR_BENCHMARK_BATCHES: u32 = 50;

// Initializes new block.
fn init_block<T: Config>()
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

    let bn = One::one();

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
    init_block::<T>();

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
            ContextSettings::new(0, 0, 0, 0, 0),
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
    }
}

fn verify_process(notes: Vec<JournalNote>) {
    assert!(
        !notes.is_empty(),
        "Journal notes cannot be empty after execution"
    );
    for note in notes {
        if let JournalNote::MessageDispatched { outcome, .. } = note {
            match outcome {
                DispatchOutcome::InitFailure { .. } | DispatchOutcome::MessageTrap { .. } => {
                    panic!("Process was not successful")
                }
                _ => {}
            }
        }
    }
}

fn run_process<T>(exec: Exec<T>) -> Vec<JournalNote>
where
    T: Config,
    T::AccountId: Origin,
{
    core_processor::process::<Externalities, ExecutionEnvironment>(
        &exec.block_config,
        exec.context,
        exec.memory_pages,
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
    memory_pages: BTreeMap<PageNumber, PageBuf>,
}

benchmarks! {

    where_clause { where
        T::AccountId: Origin,
    }

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

        #[cfg(feature = "lazy-pages")]
        type Ext = crate::ext::LazyPagesExt;

        #[cfg(not(feature = "lazy-pages"))]
        type Ext = core_processor::Ext;

        let WasmModule { code, .. } = WasmModule::<T>::sized(c * 1024, Location::Init);
    }: {
        let ext = Ext::new(default_processor_context::<T>());
        ExecutionEnvironment::new(ext, &code, Default::default(), max_pages::<T>().into()).unwrap();
    }

    claim_value {
        let caller = benchmarking::account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let program_id = benchmarking::account::<T::AccountId>("program", 0, 100);
        <T as pallet::Config>::Currency::deposit_creating(&program_id, 100_000_000_000_000_u128.unique_saturated_into());
        let code = benchmarking::generate_wasm2(16.into()).unwrap();
        benchmarking::set_program(program_id.clone().into_origin(), code, 1.into());
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

        init_block::<T>();
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
        let c in 0 .. Perbill::from_percent(49).mul_ceil(T::Schedule::get().limits.code_len);
        let value = <T as pallet::Config>::Currency::minimum_balance();
        let caller = whitelisted_caller();
        <T as pallet::Config>::Currency::make_free_balance_be(&caller, caller_funding::<T>());
        let WasmModule { code, hash: code_id, .. } = WasmModule::<T>::sized(c, Location::Handle);
        let origin = RawOrigin::Signed(caller);

        init_block::<T>();
    }: _(origin, code)
    verify {
        assert!(<T as pallet::Config>::CodeStorage::exists(code_id));
    }

    // The size of the salt influences the runtime because is is hashed in order to
    // determine the program address.
    //
    // `s`: Size of the salt in kilobytes.
    create_program {
        let s in 0 .. code::max_pages::<T>() * 64 * 128;

        let caller = whitelisted_caller();
        let origin = RawOrigin::Signed(caller);

        let WasmModule { code, hash: code_id, .. } = WasmModule::<T>::dummy();
        Gear::<T>::upload_code(origin.into(), code).expect("submit code failed");

        let salt = vec![42u8; s as usize];
        let value = <T as pallet::Config>::Currency::minimum_balance();
        let caller = whitelisted_caller();
        <T as pallet::Config>::Currency::make_free_balance_be(&caller, caller_funding::<T>());
        let origin = RawOrigin::Signed(caller);

        init_block::<T>();
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
        let c in 0 .. Perbill::from_percent(49).mul_ceil(T::Schedule::get().limits.code_len);
        let s in 0 .. code::max_pages::<T>() * 64 * 128;
        let salt = vec![42u8; s as usize];
        let value = <T as pallet::Config>::Currency::minimum_balance();
        let caller = whitelisted_caller();
        <T as pallet::Config>::Currency::make_free_balance_be(&caller, caller_funding::<T>());
        let WasmModule { code, hash, .. } = WasmModule::<T>::sized(c, Location::Handle);
        let origin = RawOrigin::Signed(caller);

        init_block::<T>();
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
        benchmarking::set_program(program_id.into_origin(), code, 1.into());
        let payload = vec![0_u8; p as usize];

        init_block::<T>();
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
        benchmarking::set_program(program_id.clone().into_origin(), code, 1.into());
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

        init_block::<T>();
    }: _(RawOrigin::Signed(caller.clone()), original_message_id, payload, 100_000_000_u64, minimum_balance)
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(Some(_))));
        assert!(MailboxOf::<T>::is_empty(&caller))
    }

    initial_allocation {
        let q in 1 .. MAX_PAGES;
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
    reinstrument {
        let c in 0 .. T::Schedule::get().limits.code_len;
        let WasmModule { code, hash, .. } = WasmModule::<T>::sized(c, Location::Handle);
        let code = Code::new_raw(code, 1, None, false).unwrap();
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
        Gear::<T>::reinstrument_code(code_id, &schedule)?;
    }

    alloc {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = alloc_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    free {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = free_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reserve_gas {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_reserve_gas_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_unreserve_gas {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_unreserve_gas_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_message_id {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = getter_bench::<T>("gr_message_id", r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_origin {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = getter_bench::<T>("gr_origin", r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_program_id {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = getter_bench::<T>("gr_program_id", r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_source {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = getter_bench::<T>("gr_source", r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_value {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = getter_bench::<T>("gr_value", r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_value_available {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = getter_bench::<T>("gr_value_available", r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_gas_available {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = number_getter_bench::<T>("gr_gas_available", ValueType::I64, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_size {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = number_getter_bench::<T>("gr_size", ValueType::I32, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_read {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_read_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_read_per_kb {
        let n in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let mut res = None;
        let exec = gr_read_per_kb_bench::<T>(n)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_block_height {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = number_getter_bench::<T>("gr_block_height", ValueType::I32, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_block_timestamp {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = number_getter_bench::<T>("gr_block_timestamp", ValueType::I64, r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_init {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_send_init_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_push {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_send_push_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_push_per_kb {
        let n in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let mut res = None;
        let exec = gr_send_push_per_kb_bench::<T>(n)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_commit {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_send_commit_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_send_commit_per_kb {
        let n in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let mut res = None;
        let exec = gr_send_commit_per_kb_bench::<T>(n)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // Benchmark the `gr_reply_commit` call.
    gr_reply_commit {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_reply_commit_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // Benchmark the `gr_reply_push` call.
    gr_reply_push {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_reply_push_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reply_push_per_kb {
        let n in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let mut res = None;
        let exec = gr_reply_push_per_kb_bench::<T>(n)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_reply_to {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_reply_to_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_debug {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_debug_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_debug_per_kb {
        let n in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let mut res = None;
        let exec = gr_debug_per_kb_bench::<T>(n)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_exit_code {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_exit_code_bench::<T>(r)?;
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
        let exec = no_return_bench::<T>("gr_exit", Some(0xff), r)?;
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
        let exec = no_return_bench::<T>("gr_leave", None, r)?;
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
        let exec = no_return_bench::<T>("gr_wait", None, r)?;
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
        let exec = no_return_bench::<T>("gr_wait_for", Some(10), r)?;
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
        let exec = no_return_bench::<T>("gr_wait_up_to", Some(100), r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_wake {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_wake_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_create_program_wgas {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let mut res = None;
        let exec = gr_create_program_wgas_bench::<T>(r)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    gr_create_program_wgas_per_kb {
        let p in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let s in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let mut res = None;
        let exec = gr_create_program_wgas_per_kb_bench::<T>(p, s)?;
    }: {
        res.replace(run_process(exec));
    }
    verify {
        verify_process(res.unwrap());
    }

    // We make the assumption that pushing a constant and dropping a value takes roughly
    // the same amount of time. We follow that `t.load` and `drop` both have the weight
    // of this benchmark / 2. We need to make this assumption because there is no way
    // to measure them on their own using a valid wasm module. We need their individual
    // values to derive the weight of individual instructions (by subtraction) from
    // benchmarks that include those for parameter pushing and return type dropping.
    // We call the weight of `t.load` and `drop`: `w_param`.
    // The weight that would result from the respective benchmark we call: `w_bench`.
    //
    // w_i{32,64}const = w_drop = w_bench / 2
    instr_i64const {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
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
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomUnaligned(0, 2 * 64 * 1024 - 8),
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
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomUnaligned(0, 2 * 64 * 1024 - 8),
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

    // w_if = w_bench - 3 * w_param
    instr_if {
        let r in 0 .. INSTR_BENCHMARK_BATCHES;
        let mut sbox = Sandbox::from(&WasmModule::<T>::from(ModuleDefinition {
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
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
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
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
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
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
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
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
            handle_body: Some(body::repeated(r * INSTR_BENCHMARK_BATCH_SIZE, &[
                Instruction::Call(OFFSET_AUX),
            ])),
            inject_stack_metering: false,
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_call_indirect = w_bench - 3 * w_param
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
            handle_body: Some(body::repeated_dyn(r * INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI32(0, num_elements as i32),
                Regular(Instruction::CallIndirect(0, 0)), // we only have one sig: 0
            ])),
            inject_stack_metering: false,
            table: Some(TableSegment {
                num_elements,
                function_index: OFFSET_AUX,
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
            handle_body: Some(body::repeated_dyn(INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI64Repeated(p as usize),
                RandomI32(0, num_elements as i32),
                Regular(Instruction::CallIndirect(p.min(1), 0)), // aux signature: 1 or 0
            ])),
            inject_stack_metering: false,
            table: Some(TableSegment {
                num_elements,
                function_index: OFFSET_AUX,
            }),
            .. Default::default()
        }));
    }: {
        sbox.invoke();
    }

    // w_local_get = w_bench - 1 * w_param
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

    // w_local_set = w_bench - 1 * w_param
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

    // w_local_tee = w_bench - 2 * w_param
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

    // w_global_get = w_bench - 1 * w_param
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

    // w_global_set = w_bench - 1 * w_param
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

    // w_memory_get = w_bench - 1 * w_param
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
