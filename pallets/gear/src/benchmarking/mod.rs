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

use self::{
    code::{
        body::{self, DynInstr::*},
        DataSegment, ImportedFunction, ImportedMemory, Location, ModuleDefinition, WasmModule,
    },
    sandbox::Sandbox,
};
use crate::{
    manager::{ExtManager, HandleKind},
    schedule::{API_BENCHMARK_BATCH_SIZE, INSTR_BENCHMARK_BATCH_SIZE},
    MailboxOf, Pallet as Gear, QueueOf, *,
};
use codec::Encode;
use common::{benchmarking, lazy_pages, storage::*, CodeMetadata, CodeStorage, Origin, ValueTree};
use core_processor::{
    common::ExecutableActor,
    configs::{AllocationsConfig, BlockInfo},
};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::{Currency, Get};
use frame_system::RawOrigin;
use gear_core::ids::{MessageId, ProgramId};
use sp_core::H256;
use sp_runtime::{
    traits::{Bounded, UniqueSaturatedInto},
    Perbill,
};
use sp_std::prelude::*;
use wasm_instrument::parity_wasm::elements::{BlockType, BrTableData, Instruction, ValueType};

const MAX_PAYLOAD_LEN: u32 = 64 * 1024;
const MAX_PAGES: u32 = 512;

/// How many batches we do per API benchmark.
const API_BENCHMARK_BATCHES: u32 = 20;

/// How many batches we do per Instruction benchmark.
const INSTR_BENCHMARK_BATCHES: u32 = 50;

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

        Gear::<T>::submit_program_raw(
            RawOrigin::Signed(caller.clone()).into(),
            module.code,
            salt,
            data,
            250_000_000_000,
            value,
        )?;

        Gear::<T>::process_queue();

        let result = Program { caller, addr };

        Ok(result)
    }
}

/// The funding that each account that either calls or instantiates programs is funded with.
fn caller_funding<T: pallet::Config>() -> BalanceOf<T> {
    BalanceOf::<T>::max_value() / 2u32.into()
}

struct Exec<T: Config> {
    ext_manager: ExtManager<T>,
    maybe_actor: Option<ExecutableActor>,
    dispatch: IncomingDispatch,
    block_info: BlockInfo,
    existential_deposit: u128,
    origin: ProgramId,
    program_id: ProgramId,
    gas_allowance: u64,
    outgoing_limit: u32,
}

fn prepare<T>(
    source: H256,
    kind: HandleKind,
    payload: Vec<u8>,
    value: u128,
) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    assert!(
        cfg!(feature = "lazy-pages") && lazy_pages::try_to_enable_lazy_pages(),
        "Suppose to run benches only with lazy pages"
    );

    let ext_manager = ExtManager::<T>::default();
    let bn: u64 = <frame_system::Pallet<T>>::block_number().unique_saturated_into();
    let root_message_id = MessageId::from(bn);

    let dispatch = match kind {
        HandleKind::Init(ref code) => {
            let program_id = ProgramId::generate(CodeId::generate(code), b"bench_salt");

            let schedule = T::Schedule::get();
            let code = Code::try_new(
                code.clone(),
                schedule.instruction_weights.version,
                |module| schedule.rules(module),
            )
            .map_err(|_| "Code failed to load: {}")?;

            let code_and_id = CodeAndId::new(code);
            let code_id = code_and_id.code_id();

            let _ = Gear::<T>::set_code_with_metadata(code_and_id, source);

            ExtManager::<T>::default().set_program(program_id, code_id, root_message_id);

            Dispatch::new(
                DispatchKind::Init,
                Message::new(
                    root_message_id,
                    ProgramId::from_origin(source),
                    program_id,
                    payload,
                    Some(u64::MAX),
                    value,
                    None,
                ),
            )
        }
        HandleKind::Handle(dest) => Dispatch::new(
            DispatchKind::Handle,
            Message::new(
                root_message_id,
                ProgramId::from_origin(source),
                dest,
                payload,
                Some(u64::MAX),
                value,
                None,
            ),
        ),
        HandleKind::Reply(msg_id, exit_code) => {
            let msg = MailboxOf::<T>::remove(<T::AccountId as Origin>::from_origin(source), msg_id)
                .map_err(|_| "Internal error: unable to find message in mailbox")?;
            Dispatch::new(
                DispatchKind::Reply,
                Message::new(
                    root_message_id,
                    ProgramId::from_origin(source),
                    msg.source(),
                    payload,
                    Some(u64::MAX),
                    value,
                    Some((msg.id(), exit_code)),
                ),
            )
        }
    };

    let initial_gas = T::BlockGasLimit::get();
    T::GasHandler::create(
        source.into_origin(),
        root_message_id.into_origin(),
        initial_gas,
    )
    .map_err(|_| "Internal error: unable to create gas handler")?;

    let dispatch = dispatch.into_stored();

    QueueOf::<T>::clear();

    QueueOf::<T>::queue(dispatch).map_err(|_| "Messages storage corrupted")?;

    let block_info = BlockInfo {
        height: <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
        timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
    };

    let existential_deposit = <T as Config>::Currency::minimum_balance().unique_saturated_into();

    if let Some(queued_dispatch) = QueueOf::<T>::dequeue().map_err(|_| "MQ storage corrupted")? {
        let actor_id = queued_dispatch.destination();
        let actor = ext_manager
            // get actor without pages data because of lazy pages enabled
            .get_executable_actor(actor_id, false)
            .ok_or("Program not found in the storage")?;
        Ok(Exec {
            ext_manager,
            maybe_actor: Some(actor),
            dispatch: queued_dispatch.into_incoming(initial_gas),
            block_info,
            existential_deposit,
            origin: ProgramId::from_origin(source),
            program_id: actor_id,
            gas_allowance: u64::MAX,
            outgoing_limit: 2048,
        })
    } else {
        Err("Dispatch not found")
    }
}

benchmarks! {

    where_clause { where
        T::AccountId: Origin,
    }

    // This constructs a program that is maximal expensive to instrument.
    // It creates a maximum number of metering blocks per byte.
    //
    // `c`: Size of the code in kilobytes.
    submit_code {
        let c in 0 .. Perbill::from_percent(49).mul_ceil(T::Schedule::get().limits.code_len);
        let value = <T as pallet::Config>::Currency::minimum_balance();
        let caller = whitelisted_caller();
        <T as pallet::Config>::Currency::make_free_balance_be(&caller, caller_funding::<T>());
        let WasmModule { code, hash: code_id, .. } = WasmModule::<T>::sized(c, Location::Handle);
        let origin = RawOrigin::Signed(caller);
    }: _(origin, code)
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
    submit_program {
        let c in 0 .. Perbill::from_percent(49).mul_ceil(T::Schedule::get().limits.code_len);
        let s in 0 .. code::max_pages::<T>() * 64 * 128;
        let salt = vec![42u8; s as usize];
        let value = <T as pallet::Config>::Currency::minimum_balance();
        let caller = whitelisted_caller();
        <T as pallet::Config>::Currency::make_free_balance_be(&caller, caller_funding::<T>());
        let WasmModule { code, hash, .. } = WasmModule::<T>::sized(c, Location::Handle);
        let origin = RawOrigin::Signed(caller);
    }: _(origin, code, salt, vec![], 100_000_000_u64, value)
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(Some(_))));
    }

    send_message {
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller = benchmarking::account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let program_id = ProgramId::from_origin(benchmarking::account::<T::AccountId>("program", 0, 100).into_origin());
        let code = benchmarking::generate_wasm2(16.into()).unwrap();
        benchmarking::set_program(program_id.into_origin(), code, 1.into());
        let payload = vec![0_u8; p as usize];
    }: _(RawOrigin::Signed(caller), program_id, payload, 100_000_000_u64, 10_000_u32.into())
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(Some(_))));
    }

    send_reply {
        let p in 0 .. MAX_PAYLOAD_LEN;
        let caller = benchmarking::account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, 100_000_000_000_000_u128.unique_saturated_into());
        let program_id = benchmarking::account::<T::AccountId>("program", 0, 100).into_origin();
        let code = benchmarking::generate_wasm2(16.into()).unwrap();
        benchmarking::set_program(program_id, code, 1.into());
        let original_message_id = benchmarking::account::<T::AccountId>("message", 0, 100).into_origin();
        MailboxOf::<T>::insert(gear_core::message::StoredMessage::new(
            MessageId::from_origin(original_message_id),
            ProgramId::from_origin(program_id),
            ProgramId::from_origin(caller.clone().into_origin()),
            Default::default(),
            0,
            None,
        )).expect("Error during mailbox insertion");
        let payload = vec![0_u8; p as usize];
    }: _(RawOrigin::Signed(caller), MessageId::from_origin(original_message_id), payload, 100_000_000_u64, 10_000_u32.into())
    verify {
        assert!(matches!(QueueOf::<T>::dequeue(), Ok(Some(_))));
    }

    initial_allocation {
        let q in 1 .. MAX_PAGES;
        let caller: T::AccountId = benchmarking::account("caller", 0, 0);
        <T as pallet::Config>::Currency::deposit_creating(&caller, (1u128 << 60).unique_saturated_into());
        let code = benchmarking::generate_wasm(q.into()).unwrap();
        let salt = vec![255u8; 32];
    }: {
        let _ = Gear::<T>::submit_program(RawOrigin::Signed(caller).into(), code, salt, vec![], 100_000_000u64, 0u32.into());
        Gear::<T>::process_queue();
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
        let _ = Gear::<T>::submit_program(RawOrigin::Signed(caller).into(), code, salt, vec![], 100_000_000u64, 0u32.into());
        Gear::<T>::process_queue();
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
            let block_number =
                <frame_system::Pallet<T>>::block_number().unique_saturated_into();
            CodeMetadata::new(caller.into_origin(), block_number)
        };

        T::CodeStorage::add_code(code_and_id, metadata).unwrap();

        let schedule = T::Schedule::get();
    }: {
        Gear::<T>::reinstrument_code(code_id, &schedule)?;
    }

    alloc {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "alloc",
                params: vec![ValueType::I32],
                return_type: Some(ValueType::I32),
            }],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(0),
                Instruction::Call(0),
                Instruction::Drop,
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    // TODO: benchmark batches and size is bigger than memory limits
    // free {
    //     let r in 0 .. API_BENCHMARK_BATCHES;
    //     let code = WasmModule::<T>::from(ModuleDefinition {
    //         memory: Some(ImportedMemory::max::<T>()),
    //         imported_functions: vec![ImportedFunction {
    //             module: "env",
    //             name: "alloc",
    //             params: vec![ValueType::I32],
    //             return_type: Some(ValueType::I32),
    //         },
    //         ImportedFunction {
    //             module: "env",
    //             name: "free",
    //             params: vec![ValueType::I32],
    //             return_type: None,
    //         }],
    //         init_body: Some(body::plain(vec![
    //             Instruction::I32Const(1),
    //             Instruction::Call(0),
    //             Instruction::Drop,
    //         ])),
    //         handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
    //             Instruction::I32Const(1),
    //             Instruction::Call(0),
    //         ])),
    //         .. Default::default()
    //     });
    //     let instance = Program::<T>::new(code, vec![])?;
    // }: {
    //     Gear::<T>::process_message(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    // }

    gas {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gas",
                params: vec![ValueType::I32],
                return_type: None,
            }],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(42),
                Instruction::Call(0),
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_gas_available {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_gas_available",
                params: vec![],
                return_type: Some(ValueType::I64),
            }],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::Call(0),
                Instruction::Drop,
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_msg_id {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let instance = Program::<T>::new(WasmModule::getter(
            "env", "gr_msg_id", r * API_BENCHMARK_BATCH_SIZE
        ), vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_origin {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let instance = Program::<T>::new(WasmModule::getter(
            "env", "gr_origin", r * API_BENCHMARK_BATCH_SIZE
        ), vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_program_id {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let instance = Program::<T>::new(WasmModule::getter(
            "env", "gr_program_id", r * API_BENCHMARK_BATCH_SIZE
        ), vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_source {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let instance = Program::<T>::new(WasmModule::getter(
            "env", "gr_source", r * API_BENCHMARK_BATCH_SIZE
        ), vec![])?;

        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;

    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_value {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let instance = Program::<T>::new(WasmModule::getter(
            "env", "gr_value", r * API_BENCHMARK_BATCH_SIZE
        ), vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_value_available {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let instance = Program::<T>::new(WasmModule::getter(
            "env", "gr_value_available", r * API_BENCHMARK_BATCH_SIZE
        ), vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_size {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_size",
                params: vec![],
                return_type: Some(ValueType::I32),
            }],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::Call(0),
                Instruction::Drop,
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_read {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let pages = 1u32;
        let buffer_size = pages * 64 * 1024 - 4;
        let instance = Program::<T>::new(WasmModule::<T>::dummy(), vec![])?;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_read",
                params: vec![ValueType::I32, ValueType::I32, ValueType::I32],
                return_type: None,
            }],
            data_segments: vec![
                DataSegment {
                    offset: 0,
                    value: buffer_size.to_le_bytes().to_vec(),
                },
            ],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(0), // at
                Instruction::I32Const(0), // len
                Instruction::I32Const(0), // output ptr
                Instruction::Call(0),
                ])),
                .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_read_per_kb {
        let n in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let pages = 16u32;
        let buffer_size = pages * 64 * 1024 - 4;
        let instance = Program::<T>::new(WasmModule::<T>::dummy(), vec![])?;
        let pid_bytes = instance.addr.encode();
        let pid_len = pid_bytes.len();
        let value_bytes = 0_u128.encode();
        let value_len = value_bytes.len();
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_read",
                params: vec![ValueType::I32, ValueType::I32, ValueType::I32],
                return_type: None,
            }],
            data_segments: vec![
                DataSegment {
                    offset: 0,
                    value: buffer_size.to_le_bytes().to_vec(),
                },
            ],
            handle_body: Some(body::repeated(API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(0), // at
                Instruction::I32Const((n * 1024) as i32), // len
                Instruction::I32Const(0), // output ptr
                Instruction::Call(0),
                ])),
                .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![0xff; (n * 1024) as usize], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_block_height {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_block_height",
                params: vec![],
                return_type: Some(ValueType::I32),
            }],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::Call(0),
                Instruction::Drop,
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_block_timestamp {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_block_timestamp",
                params: vec![],
                return_type: Some(ValueType::I64),
            }],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::Call(0),
                Instruction::Drop,
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_send_init {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_send_init",
                params: vec![ValueType::I32],
                return_type: Some(ValueType::I32),
            }],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(0), // handle ptr
                Instruction::Call(0),
                Instruction::Drop,
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
        core_processor::handle_journal(journal, &mut ext_manager);
    }

    gr_send_push {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_send_init",
                params: vec![ValueType::I32],
                return_type: Some(ValueType::I32),
            },
            ImportedFunction {
                module: "env",
                name: "gr_send_push",
                params: vec![ValueType::I32, ValueType::I32, ValueType::I32],
                return_type: Some(ValueType::I32),
            }],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(0), // handle ptr
                Instruction::Call(0), // get handle
                Instruction::Drop,
                Instruction::I32Const(0), // handle ptr
                Instruction::I32Const(0), // payload ptr
                Instruction::I32Const(0), // payload len
                Instruction::Call(1), // send_push
                Instruction::Drop,
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
        core_processor::handle_journal(journal, &mut ext_manager);
    }

    gr_send_push_per_kb {
        let n in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_send_init",
                params: vec![ValueType::I32],
                return_type: Some(ValueType::I32),
            },
            ImportedFunction {
                module: "env",
                name: "gr_send_push",
                params: vec![ValueType::I32, ValueType::I32, ValueType::I32],
                return_type: Some(ValueType::I32),
            }],
            handle_body: Some(body::repeated(API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(0), // handle ptr
                Instruction::Call(0), // get handle
                Instruction::Drop,
                Instruction::I32Const(0), // handle ptr
                Instruction::I32Const(0), // payload ptr
                Instruction::I32Const((n * 1024) as i32), // payload_len
                Instruction::Call(1), // send_push
                Instruction::Drop,
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
        core_processor::handle_journal(journal, &mut ext_manager);
    }

    // Benchmark the `gr_send_commit` call.
    // `gr_send` call is shortcut for `gr_send_init` + `gr_send_commit`
    gr_send_commit {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let instance = Program::<T>::new(WasmModule::<T>::dummy(), vec![])?;
        let pid_bytes = instance.addr.encode();
        let pid_len = pid_bytes.len();
        let value_bytes = 0_u128.encode();
        let value_len = value_bytes.len();
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_send",
                params: vec![ValueType::I32, ValueType::I32, ValueType::I32, ValueType::I32, ValueType::I32],
                return_type: Some(ValueType::I32),
            }],
            data_segments: vec![
                DataSegment {
                    offset: 0_u32,
                    value: pid_bytes,
                },
                DataSegment {
                    offset: pid_len as u32,
                    value: value_bytes,
                },
            ],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCHES, &[
                Instruction::I32Const(0), // program_id_ptr
                Instruction::I32Const(0), // payload_ptr
                Instruction::I32Const(0), // payload_len
                Instruction::I32Const(pid_len as i32), // value_ptr
                Instruction::I32Const((pid_len + value_len) as i32), // message_id_ptr
                Instruction::Call(0),
                Instruction::Drop,
                ])),
                .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;

        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 10000000u32.into())?;
    }: {

        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );

        core_processor::handle_journal(journal, &mut ext_manager);
    }

    // Benchmark the `gr_send_commit` call.
    // `n`: Size of message payload in kb
    gr_send_commit_per_kb {
        let n in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let instance = Program::<T>::new(WasmModule::<T>::dummy(), vec![])?;
        let pid_bytes = instance.addr.encode();
        let pid_len = pid_bytes.len();
        let value_bytes = 0_u128.encode();
        let value_len = value_bytes.len();
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_send",
                params: vec![ValueType::I32, ValueType::I32, ValueType::I32, ValueType::I32, ValueType::I32],
                return_type: Some(ValueType::I32),
            }],
            data_segments: vec![
                DataSegment {
                    offset: 0_u32,
                    value: pid_bytes,
                },
                DataSegment {
                    offset: pid_len as u32,
                    value: value_bytes,
                },
            ],
            handle_body: Some(body::plain(vec![
                Instruction::I32Const(0), // program_id_ptr
                Instruction::I32Const(0), // payload_ptr
                Instruction::I32Const((n * 1024) as i32), // payload_len
                Instruction::I32Const(pid_len as i32), // value_ptr
                Instruction::I32Const((pid_len + value_len) as i32), // message_id_ptr
                Instruction::Call(0),
                Instruction::Drop,
                Instruction::End,
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 10000000u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );

        core_processor::handle_journal(journal, &mut ext_manager);
    }

    // Benchmark the `gr_reply_commit` call.
    gr_reply_commit {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let instance = Program::<T>::new(WasmModule::<T>::dummy(), vec![])?;
        let value_bytes = 0_u128.encode();
        let value_len = value_bytes.len();
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_reply_commit",
                params: vec![ValueType::I32, ValueType::I32, ValueType::I32],
                return_type: Some(ValueType::I32),
            }],
            data_segments: vec![
                DataSegment {
                    offset: 0u32,
                    value: value_bytes,
                },
            ],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(0), // payload_ptr
                Instruction::I32Const(0), // payload_len
                Instruction::I32Const(0), // value_ptr
                Instruction::Call(0),
                Instruction::Drop,
                ])),
                .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 10000000u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );

        core_processor::handle_journal(journal, &mut ext_manager);
    }

    gr_reply_commit_per_kb {
        let n in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let value_bytes = 0_u128.encode();
        let value_len = value_bytes.len();
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_reply_commit",
                params: vec![ValueType::I32, ValueType::I32, ValueType::I32],
                return_type: Some(ValueType::I32),
            }],
            data_segments: vec![
                DataSegment {
                    offset: 0u32,
                    value: value_bytes,
                },
            ],
            handle_body: Some(body::repeated(API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(0), // payload_ptr
                Instruction::I32Const((n * 1024) as i32), // payload_len
                Instruction::I32Const(0), // value_ptr
                Instruction::Call(0),
                Instruction::Drop,
                ])),
                .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 10000000u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );

        core_processor::handle_journal(journal, &mut ext_manager);
    }

    // Benchmark the `gr_reply_push` call.
    gr_reply_push {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let instance = Program::<T>::new(WasmModule::<T>::dummy(), vec![])?;
        let value_bytes = 0_u128.encode();
        let value_len = value_bytes.len();
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_reply_push",
                params: vec![ValueType::I32, ValueType::I32],
                return_type: Some(ValueType::I32),
            }],
            data_segments: vec![
                DataSegment {
                    offset: 0u32,
                    value: value_bytes,
                },
            ],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(0), // payload_ptr
                Instruction::I32Const(0), // payload_len
                Instruction::Call(0),
                Instruction::Drop,
                ])),
                .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 10000000u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_reply_push_per_kb {
        let n in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let value_bytes = 0_u128.encode();
        let value_len = value_bytes.len();
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_reply_push",
                params: vec![ValueType::I32, ValueType::I32],
                return_type: Some(ValueType::I32),
            }],
            data_segments: vec![
                DataSegment {
                    offset: 0u32,
                    value: value_bytes,
                },
            ],
            handle_body: Some(body::repeated(API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(0), // payload_ptr
                Instruction::I32Const((n * 1024) as i32), // payload_len
                Instruction::Call(0),
                Instruction::Drop,
                ])),
                .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 10000000u32.into())?;
    }: {
        core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
    }

    gr_reply_to {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_reply_to",
                params: vec![ValueType::I32],
                return_type: None,
            }],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(0), // dest_ptr
                Instruction::Call(0),
                ])),
                .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let msg_id = MessageId::from(10);
        let msg = gear_core::message::Message::new(msg_id, instance.addr.as_bytes().into(), ProgramId::from(instance.caller.clone().into_origin().as_bytes()), vec![], Some(1_000_000), 0, None).into_stored();
        MailboxOf::<T>::insert(msg).expect("Error during mailbox insertion");
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Reply(msg_id, 0), vec![], 0u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );

        core_processor::handle_journal(journal, &mut ext_manager);
    }

    gr_debug {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_debug",
                params: vec![ValueType::I32, ValueType::I32],
                return_type: None,
            }],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::I32Const(0),
                Instruction::I32Const(0),
                Instruction::Call(0),
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );

        core_processor::handle_journal(journal, &mut ext_manager);
    }

    gr_exit_code {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_exit_code",
                params: vec![],
                return_type: Some(ValueType::I32),
            }],
            handle_body: Some(body::repeated(r * API_BENCHMARK_BATCH_SIZE, &[
                Instruction::Call(0),
                Instruction::Drop,
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let msg_id = MessageId::from(10);
        let msg = gear_core::message::Message::new(msg_id, instance.addr.as_bytes().into(), ProgramId::from(instance.caller.clone().into_origin().as_bytes()), vec![], Some(1_000_000), 0, None).into_stored();
        MailboxOf::<T>::insert(msg).expect("Error during mailbox insertion");
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Reply(msg_id, 0), vec![], 0u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );

        core_processor::handle_journal(journal, &mut ext_manager);
    }

    // We cannot call `gr_exit` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_exit {
        let r in 0 .. 1;
        let pid_bytes = ProgramId::from(1).encode();
        let pid_len = pid_bytes.len();
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_exit",
                params: vec![ValueType::I32],
                return_type: None,
            }],
            data_segments: vec![
                DataSegment {
                    offset: 0_u32,
                    value: pid_bytes,
                },
            ],
            handle_body: Some(body::repeated(r, &[
                Instruction::I32Const(0),
                Instruction::Call(0),
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
        core_processor::handle_journal(journal, &mut ext_manager);
    }

    // We cannot call `gr_leave` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_leave {
        let r in 0 .. 1;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_leave",
                params: vec![],
                return_type: None,
            }],
            handle_body: Some(body::repeated(r, &[
                Instruction::Call(0),
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
        core_processor::handle_journal(journal, &mut ext_manager);
    }

    // We cannot call `gr_wait` multiple times. Therefore our weight determination is not
    // as precise as with other APIs.
    gr_wait {
        let r in 0 .. 1;
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_wait",
                params: vec![],
                return_type: None,
            }],
            handle_body: Some(body::repeated(r, &[
                Instruction::Call(0),
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
        core_processor::handle_journal(journal, &mut ext_manager);
    }

    gr_wake {
        let r in 0 .. API_BENCHMARK_BATCHES;
        let message_ids = (0..r * API_BENCHMARK_BATCH_SIZE)
            .map(|i| gear_core::ids::MessageId::from(i as u64))
            .collect::<Vec<_>>();
        let message_id_len = message_ids.get(0).map(|i| i.encode().len()).unwrap_or(0);
        let message_id_bytes = message_ids.iter().flat_map(|x| x.encode()).collect();
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_wake",
                params: vec![ValueType::I32],
                return_type: None,
            }],
            data_segments: vec![
                DataSegment {
                    offset: 0_u32,
                    value: message_id_bytes,
                },
            ],
            handle_body: Some(body::repeated_dyn(r * API_BENCHMARK_BATCH_SIZE, vec![
                Counter(0_u32, message_id_len as u32), // message_id_ptr
                Regular(Instruction::Call(0)),
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        for message_id in message_ids {
            let message = gear_core::message::Message::new(message_id, 1.into(), ProgramId::from(instance.addr.as_bytes()), vec![], Some(1_000_000), 0, None);
            let dispatch = gear_core::message::Dispatch::new(gear_core::message::DispatchKind::Handle, message).into_stored();
            WaitlistOf::<T>::insert(dispatch.clone()).expect("Duplicate wl message");
        }
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
        core_processor::handle_journal(journal, &mut ext_manager);
    }

    gr_create_program_wgas {
        let r in 0 .. 1;
        let module = WasmModule::<T>::dummy();
        let code_hash_bytes = module.hash.encode();
        let code_hash_len = code_hash_bytes.len();
        let salt_bytes = r.encode();
        let salt_bytes_len = salt_bytes.len();
        let value_bytes = 0_u128.encode();
        let value_bytes_len = value_bytes.len();
        let pid_bytes = ProgramId::from(101).encode();
        let _ = Gear::<T>::submit_code_raw(RawOrigin::Signed(benchmarking::account("instantiator", 0, 0)).into(), module.code);
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_create_program_wgas",
                params: vec![ValueType::I32, ValueType::I32, ValueType::I32, ValueType::I32, ValueType::I32, ValueType::I64, ValueType::I32, ValueType::I32],
                return_type: None,
            }],
            data_segments: vec![
                DataSegment {
                    offset: 0_u32,
                    value: code_hash_bytes,
                },
                DataSegment {
                    offset: code_hash_len as u32,
                    value: salt_bytes,
                },
                DataSegment {
                    offset: (salt_bytes_len + code_hash_len) as u32,
                    value: value_bytes,
                },
                DataSegment {
                    offset: (value_bytes_len + salt_bytes_len + code_hash_len) as u32,
                    value: pid_bytes,
                },
            ],
            handle_body: Some(body::repeated_dyn(r, vec![
                Regular(Instruction::I32Const(0)),
                Regular(Instruction::I32Const(code_hash_len as i32)),
                Counter(0_u32, r as u32), // salt len
                Regular(Instruction::I32Const(0)),
                Regular(Instruction::I32Const(0)), // payload_len
                Regular(Instruction::I64Const(100000000)),
                Regular(Instruction::I32Const((salt_bytes_len + code_hash_len) as i32)),
                Regular(Instruction::I32Const((value_bytes_len + salt_bytes_len + code_hash_len) as i32)),
                Regular(Instruction::Call(0)),
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
        core_processor::handle_journal(journal, &mut ext_manager);

    }

    gr_create_program_wgas_per_kb {
        let n in 0 .. T::Schedule::get().limits.payload_len / 1024;
        let module = WasmModule::<T>::dummy();
        let code_hash_bytes = module.hash.encode();
        let code_hash_len = code_hash_bytes.len();
        let salt_bytes = n.encode();
        let salt_bytes_len = salt_bytes.len();
        let value_bytes = 0_u128.encode();
        let value_bytes_len = value_bytes.len();
        let pid_bytes = ProgramId::from(101).encode();
        let _ = Gear::<T>::submit_code_raw(RawOrigin::Signed(benchmarking::account("instantiator", 0, 0)).into(), module.code);
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![ImportedFunction {
                module: "env",
                name: "gr_create_program_wgas",
                params: vec![ValueType::I32, ValueType::I32, ValueType::I32, ValueType::I32, ValueType::I32, ValueType::I64, ValueType::I32, ValueType::I32],
                return_type: None,
            }],
            data_segments: vec![
                DataSegment {
                    offset: 0_u32,
                    value: code_hash_bytes,
                },
                DataSegment {
                    offset: code_hash_len as u32,
                    value: salt_bytes,
                },
                DataSegment {
                    offset: (salt_bytes_len + code_hash_len) as u32,
                    value: value_bytes,
                },
                DataSegment {
                    offset: (value_bytes_len + salt_bytes_len + code_hash_len) as u32,
                    value: pid_bytes,
                },
            ],
            handle_body: Some(body::repeated_dyn(API_BENCHMARK_BATCH_SIZE, vec![
                Regular(Instruction::I32Const(0)),
                Regular(Instruction::I32Const(code_hash_len as i32)),
                Counter(0_u32, API_BENCHMARK_BATCH_SIZE as u32), // salt len
                Regular(Instruction::I32Const(0)),
                Regular(Instruction::I32Const((n * 1024) as i32)), // payload_len
                Regular(Instruction::I64Const(100000000)),
                Regular(Instruction::I32Const((salt_bytes_len + code_hash_len) as i32)),
                Regular(Instruction::I32Const((value_bytes_len + salt_bytes_len + code_hash_len) as i32)),
                Regular(Instruction::Call(0)),
            ])),
            .. Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let Exec {
            mut ext_manager,
            maybe_actor,
            dispatch,
            block_info,
            existential_deposit,
            origin,
            program_id,
            gas_allowance,
            outgoing_limit,
        } = prepare::<T>(instance.caller.into_origin(), HandleKind::Handle(ProgramId::from_origin(instance.addr)), vec![], 0u32.into())?;
    }: {
        let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_actor,
                    dispatch,
                    block_info,
                    AllocationsConfig {
                        max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
                        init_cost: T::Schedule::get().memory_weights.initial_cost,
                        alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
                        mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
                        load_page_cost: T::Schedule::get().memory_weights.load_cost,
                    },
                    existential_deposit,
                    origin,
                    program_id,
                    gas_allowance,
                    outgoing_limit,
                    Default::default(),
                    Default::default(),
                );
        core_processor::handle_journal(journal, &mut ext_manager);

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
                Instruction::Call(2), // call aux
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
            handle_body: Some(body::repeated_dyn(INSTR_BENCHMARK_BATCH_SIZE, vec![
                RandomI64Repeated(p as usize),
                RandomI32(0, num_elements as i32),
                Regular(Instruction::CallIndirect(p.min(1), 0)), // aux signature: 1 or 0
            ])),
            inject_stack_metering: false,
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
        let max_locals = T::Schedule::get().limits.stack_height;
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
        let max_locals = T::Schedule::get().limits.stack_height;
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
