use super::{
    code::{
        body::{self, DynInstr::*},
        max_pages, DataSegment, ImportedFunction, ImportedMemory, Location, ModuleDefinition,
        WasmModule, OFFSET_AUX,
    },
    sandbox::Sandbox,
};
use crate::{
    manager::{CodeInfo, ExtManager, HandleKind},
    pallet,
    schedule::{API_BENCHMARK_BATCH_SIZE, INSTR_BENCHMARK_BATCH_SIZE},
    BTreeMap, BalanceOf, BlockGasLimitOf, Call, Config, CostsPerBlockOf, CurrencyOf, DbWeightOf,
    ExecutionEnvironment, Ext as Externalities, GasHandlerOf, MailboxOf, Pallet as Gear, Pallet,
    QueueOf, ReadPerByteCostOf, Schedule, WaitlistOf,
};
use codec::Encode;
use common::{
    benchmarking, scheduler::SchedulingCostsPerBlock, storage::*, CodeMetadata, CodeStorage,
    GasPrice, GasTree, Origin,
};
use core::mem::size_of;
use core_processor::{
    common::{DispatchOutcome, JournalNote},
    configs::{AllocationsConfig, BlockConfig, BlockInfo, MessageExecutionContext},
    PrechargeResult, PrepareResult, ProcessExecutionContext, ProcessorContext, ProcessorExt,
};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::{Currency, Get, Hooks, ReservableCurrency};
use frame_system::{Pallet as SystemPallet, RawOrigin};
use gear_backend_common::Environment;
use gear_core::{
    code::{Code, CodeAndId},
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    ids::{CodeId, MessageId, ProgramId},
    memory::{AllocationsContext, PageBuf, PageNumber, WasmPageNumber},
    message::{ContextSettings, Dispatch, DispatchKind, Message, MessageContext, ReplyDetails},
    reservation::GasReserver,
};
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
use sp_std::{convert::TryInto, prelude::*};
use wasm_instrument::parity_wasm::elements::{BlockType, BrTableData, Instruction, ValueType};

use super::{Exec, Program};

const BAD_OFFSET: u32 = PageNumber::size() as u32 - 1;

fn safe_offset_delta(max_pages: u32, data_size: u32, r: u32) -> u32 {
    if r == 0 {
        0
    } else {
        ((max_pages - 2) * WasmPageNumber::size() as u32 - data_size) / r
    }
}

pub fn prepare<T>(
    source: H256,
    kind: HandleKind,
    payload: Vec<u8>,
    value: u128,
) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    #[cfg(feature = "lazy-pages")]
    assert!(gear_lazy_pages_common::try_to_enable_lazy_pages());

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
                schedule.limits.stack_height,
            )
            .map_err(|_| "Code failed to load")?;

            let code_and_id = CodeAndId::new(code);
            let code_info = CodeInfo::from_code_and_id(&code_and_id);

            let _ = Gear::<T>::set_code_with_metadata(code_and_id, source);

            ExtManager::<T>::default().set_program(program_id, &code_info, root_message_id);

            Dispatch::new(
                DispatchKind::Init,
                Message::new(
                    root_message_id,
                    ProgramId::from_origin(source),
                    program_id,
                    payload.try_into()?,
                    Some(u64::MAX),
                    value,
                    None,
                ),
            )
        }
        HandleKind::InitByHash(code_id) => {
            let program_id = ProgramId::generate(code_id, b"bench_salt");

            let code = T::CodeStorage::get_code(code_id).ok_or("Code not found in storage")?;
            let code_info = CodeInfo::from_code(&code_id, &code);

            ExtManager::<T>::default().set_program(program_id, &code_info, root_message_id);

            Dispatch::new(
                DispatchKind::Init,
                Message::new(
                    root_message_id,
                    ProgramId::from_origin(source),
                    program_id,
                    payload.try_into()?,
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
                payload.try_into()?,
                Some(u64::MAX),
                value,
                None,
            ),
        ),
        HandleKind::Reply(msg_id, exit_code) => {
            let (msg, _bn) =
                MailboxOf::<T>::remove(<T::AccountId as Origin>::from_origin(source), msg_id)
                    .map_err(|_| "Internal error: unable to find message in mailbox")?;
            Dispatch::new(
                DispatchKind::Reply,
                Message::new(
                    root_message_id,
                    ProgramId::from_origin(source),
                    msg.source(),
                    payload.try_into()?,
                    Some(u64::MAX),
                    value,
                    Some(ReplyDetails::new(msg.id(), exit_code)),
                ),
            )
        }
    };

    let initial_gas = BlockGasLimitOf::<T>::get();
    let origin = <T::AccountId as Origin>::from_origin(source);
    GasHandlerOf::<T>::create(origin, root_message_id, initial_gas)
        .map_err(|_| "Internal error: unable to create gas handler")?;

    let dispatch = dispatch.into_stored();

    QueueOf::<T>::clear();

    QueueOf::<T>::queue(dispatch).map_err(|_| "Messages storage corrupted")?;

    let block_info = BlockInfo {
        height: <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
        timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
    };

    let existential_deposit = CurrencyOf::<T>::minimum_balance().unique_saturated_into();
    let mailbox_threshold = <T as Config>::MailboxThreshold::get();
    let waitlist_cost = CostsPerBlockOf::<T>::waitlist();
    let reserve_for = CostsPerBlockOf::<T>::reserve_for().unique_saturated_into();
    let reservation = CostsPerBlockOf::<T>::reservation().unique_saturated_into();

    let block_config = BlockConfig {
        block_info,
        allocations_config: AllocationsConfig {
            max_pages: gear_core::memory::WasmPageNumber(T::Schedule::get().limits.memory_pages),
            init_cost: T::Schedule::get().memory_weights.initial_cost,
            alloc_cost: T::Schedule::get().memory_weights.allocation_cost,
            mem_grow_cost: T::Schedule::get().memory_weights.grow_cost,
            load_page_cost: T::Schedule::get().memory_weights.load_cost,
        },
        existential_deposit,
        outgoing_limit: 2048,
        host_fn_weights: Default::default(),
        forbidden_funcs: Default::default(),
        mailbox_threshold,
        waitlist_cost,
        reserve_for,
        reservation,
        read_cost: DbWeightOf::<T>::get().reads(1).ref_time(),
        write_cost: DbWeightOf::<T>::get().writes(1).ref_time(),
        write_per_byte_cost: schedule.db_write_per_byte,
        read_per_byte_cost: schedule.db_read_per_byte,
        module_instantiation_byte_cost: schedule.module_instantiation_per_byte,
        max_reservations: T::ReservationsLimit::get(),
    };

    if let Some(queued_dispatch) = QueueOf::<T>::dequeue().map_err(|_| "MQ storage corrupted")? {
        let actor_id = queued_dispatch.destination();
        let actor = ext_manager
            .get_actor(actor_id)
            .ok_or("Program not found in the storage")?;

        let precharged_dispatch = match core_processor::precharge(
            &block_config,
            u64::MAX,
            queued_dispatch.into_incoming(initial_gas),
            actor_id,
        ) {
            PrechargeResult::Ok(d) => d,
            PrechargeResult::Error(_) => {
                return Err("core_processor::precharge failed");
            }
        };

        let message_execution_context = MessageExecutionContext {
            actor,
            precharged_dispatch,
            origin: ProgramId::from_origin(source),
            subsequent_execution: false,
        };

        let (context, code) =
            match core_processor::prepare(&block_config, message_execution_context) {
                PrepareResult::Ok(context) => {
                    let code = T::CodeStorage::get_code(context.actor_data().code_id)
                        .ok_or("Program code not found")?;

                    (context, code)
                }
                _ => return Err("core_processor::prepare failed"),
            };

        Ok(Exec {
            ext_manager,
            block_config,
            context: (context, actor_id, code).into(),
            // actor without pages data because of lazy pages enabled
            memory_pages: Default::default(),
        })
    } else {
        Err("Dispatch not found")
    }
}

pub fn alloc_bench<T>(r: u32) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory { min_pages: 0 }),
        imported_functions: vec![ImportedFunction {
            module: "env",
            name: "alloc",
            params: vec![ValueType::I32],
            return_type: Some(ValueType::I32),
        }],
        handle_body: Some(body::repeated(
            r * API_BENCHMARK_BATCH_SIZE,
            &[
                Instruction::I32Const(0),
                Instruction::Call(0),
                Instruction::Drop,
            ],
        )),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        vec![],
        0u32.into(),
    )
}

pub fn free_bench<T>(r: u32) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    assert!(r <= max_pages::<T>());

    use Instruction::*;
    let mut instructions = vec![];
    for _ in 0..API_BENCHMARK_BATCH_SIZE {
        instructions.push(I32Const(r as i32));
        instructions.push(Call(0));
        instructions.push(Drop);
        for page in 0..r {
            instructions.push(I32Const(page as i32));
            instructions.push(Call(1));
        }
    }
    instructions.push(End);

    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory { min_pages: 0 }),
        imported_functions: vec![
            ImportedFunction {
                module: "env",
                name: "alloc",
                params: vec![ValueType::I32],
                return_type: Some(ValueType::I32),
            },
            ImportedFunction {
                module: "env",
                name: "free",
                params: vec![ValueType::I32],
                return_type: None,
            },
        ],
        init_body: None,
        handle_body: Some(body::plain(instructions)),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        vec![],
        0u32.into(),
    )
}

pub fn gr_gas_available_bench<T>(r: u32) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        imported_functions: vec![ImportedFunction {
            module: "env",
            name: "gr_gas_available",
            params: vec![],
            return_type: Some(ValueType::I64),
        }],
        handle_body: Some(body::repeated(
            r * API_BENCHMARK_BATCH_SIZE,
            &[Instruction::Call(0), Instruction::Drop],
        )),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        vec![],
        0u32.into(),
    )
}

pub fn gr_reserve_gas_bench<T>(r: u32) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    let id_bytes = u128::MAX.encode();

    let id_offset = BAD_OFFSET;

    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        imported_functions: vec![ImportedFunction {
            module: "env",
            name: "gr_reserve_gas",
            params: vec![ValueType::I64, ValueType::I32, ValueType::I32],
            return_type: Some(ValueType::I32),
        }],
        data_segments: vec![DataSegment {
            offset: id_offset,
            value: id_bytes,
        }],
        handle_body: Some(body::repeated(
            r * API_BENCHMARK_BATCH_SIZE,
            &[
                Instruction::I64Const(50_000_000),       // gas amount
                Instruction::I32Const(10),               // duration
                Instruction::I32Const(id_offset as i32), // id ptr
                Instruction::Call(0),
                Instruction::Drop,
            ],
        )),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        vec![],
        0u32.into(),
    )
}

pub fn gr_unreserve_gas_bench<T>(r: u32) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    assert!(r <= 1);

    let id_bytes = 0_u128.encode();
    let amount_bytes = 0_u64.encode();

    let id_offset = BAD_OFFSET;
    let amount_offset = BAD_OFFSET + WasmPageNumber::size() as u32;

    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        imported_functions: vec![ImportedFunction {
            module: "env",
            name: "gr_unreserve_gas",
            params: vec![ValueType::I32, ValueType::I32],
            return_type: Some(ValueType::I32),
        }],
        data_segments: vec![
            DataSegment {
                offset: id_offset,
                value: id_bytes,
            },
            DataSegment {
                offset: amount_offset,
                value: amount_bytes,
            },
        ],
        handle_body: Some(body::repeated(
            r,
            &[
                Instruction::I32Const(id_offset as i32),     // id ptr
                Instruction::I32Const(amount_offset as i32), // unreserved amount ptr
                Instruction::Call(0),
                Instruction::Drop,
            ],
        )),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        vec![],
        0u32.into(),
    )
}

pub fn getter_bench<T>(name: &'static str, r: u32) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    let r = r * API_BENCHMARK_BATCH_SIZE;
    let offset = BAD_OFFSET;
    let offset_delta = safe_offset_delta(max_pages::<T>(), 0x1000, r);
    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        imported_functions: vec![ImportedFunction {
            module: "env",
            name,
            params: vec![ValueType::I32],
            return_type: None,
        }],
        handle_body: Some(body::repeated_dyn(
            r,
            vec![
                Counter(offset, offset_delta), // ptr where to store output
                Regular(Instruction::Call(0)), // call the imported function
            ],
        )),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        vec![],
        0u32.into(),
    )
}

pub fn number_getter_bench<T>(
    name: &'static str,
    return_type: ValueType,
    r: u32,
) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        imported_functions: vec![ImportedFunction {
            module: "env",
            name,
            params: vec![],
            return_type: Some(return_type),
        }],
        handle_body: Some(body::repeated(
            r * API_BENCHMARK_BATCH_SIZE,
            &[Instruction::Call(0), Instruction::Drop],
        )),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        vec![],
        0u32.into(),
    )
}

pub fn gr_read_bench<T>(r: u32) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    let r = r * API_BENCHMARK_BATCH_SIZE;
    let payload = vec![1u8; 100];
    let offset = BAD_OFFSET;
    let offset_delta = safe_offset_delta(max_pages::<T>(), payload.len() as u32, r);
    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        imported_functions: vec![ImportedFunction {
            module: "env",
            name: "gr_read",
            params: vec![ValueType::I32, ValueType::I32, ValueType::I32],
            return_type: Some(ValueType::I32),
        }],
        handle_body: Some(body::repeated_dyn(
            r,
            vec![
                Regular(Instruction::I32Const(0)),  // at
                Regular(Instruction::I32Const(10)), // len
                Counter(offset, offset_delta),      // buffer ptr
                Regular(Instruction::Call(0)),
                Regular(Instruction::Drop),
            ],
        )),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        payload,
        0u32.into(),
    )
}

pub fn gr_read_per_kb_bench<T>(n: u32) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    let r = API_BENCHMARK_BATCH_SIZE;
    let payload = vec![0xff; (n * 1024) as usize];
    let offset = BAD_OFFSET;
    let offset_delta = safe_offset_delta(max_pages::<T>(), payload.len() as u32, r);
    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        imported_functions: vec![ImportedFunction {
            module: "env",
            name: "gr_read",
            params: vec![ValueType::I32, ValueType::I32, ValueType::I32],
            return_type: Some(ValueType::I32),
        }],
        handle_body: Some(body::repeated_dyn(
            r,
            vec![
                Regular(Instruction::I32Const(0)),                    // at
                Regular(Instruction::I32Const(payload.len() as i32)), // len
                Counter(offset, offset_delta),                        // buffer ptr
                Regular(Instruction::Call(0)),
                Regular(Instruction::Drop),
            ],
        )),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        payload,
        0u32.into(),
    )
}

pub fn gr_send_init_bench<T>(r: u32) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        imported_functions: vec![ImportedFunction {
            module: "env",
            name: "gr_send_init",
            params: vec![ValueType::I32],
            return_type: Some(ValueType::I32),
        }],
        handle_body: Some(body::repeated(
            r * API_BENCHMARK_BATCH_SIZE,
            &[
                Instruction::I32Const(0), // handle
                Instruction::Call(0),
                Instruction::Drop,
            ],
        )),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        vec![],
        0u32.into(),
    )
}

pub fn gr_send_push_bench<T>(r: u32) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    let r = r * API_BENCHMARK_BATCH_SIZE;
    let payload = vec![1u8; 100];
    let offset = BAD_OFFSET;
    let offset_delta = safe_offset_delta(max_pages::<T>(), payload.len() as u32, r);
    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        imported_functions: vec![
            ImportedFunction {
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
            },
        ],
        handle_body: Some(body::repeated_dyn(
            r,
            vec![
                Regular(Instruction::I32Const(0)), // handle
                Regular(Instruction::Call(0)),
                Regular(Instruction::Drop),
                Regular(Instruction::I32Const(0)), // message handle
                Counter(offset, offset_delta),     // payload ptr
                Regular(Instruction::I32Const(100)), // payload len
                Regular(Instruction::Call(1)),
                Regular(Instruction::Drop),
            ],
        )),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        vec![],
        0u32.into(),
    )
}

pub fn gr_send_push_per_kb_bench<T>(n: u32) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    let r = API_BENCHMARK_BATCH_SIZE;
    let payload_len = n * 1024;
    let offset = BAD_OFFSET;
    let offset_delta = safe_offset_delta(max_pages::<T>(), payload_len, r);
    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        imported_functions: vec![
            ImportedFunction {
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
            },
        ],
        handle_body: Some(body::repeated_dyn(
            r,
            vec![
                Regular(Instruction::I32Const(0)), // handle
                Regular(Instruction::Call(0)),
                Regular(Instruction::Drop),
                Regular(Instruction::I32Const(0)), // message handle
                Counter(offset, offset_delta),     // payload ptr
                Regular(Instruction::I32Const(payload_len as i32)), // payload len
                Regular(Instruction::Call(1)),
                Regular(Instruction::Drop),
            ],
        )),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        vec![],
        0u32.into(),
    )
}

// Benchmark the `gr_send_commit` call.
// `gr_send` call is shortcut for `gr_send_init` + `gr_send_commit`
pub fn gr_send_commit_bench<T>(r: u32) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    let r = r * API_BENCHMARK_BATCH_SIZE;

    let dest_offset = 1;
    let value_offset = 0xff;
    let message_id_offset = 0x1ff;
    let payload_offset = BAD_OFFSET;
    let payload_len = 100;

    let offset_delta = safe_offset_delta(max_pages::<T>(), payload_len, r);

    let code = WasmModule::<T>::from(ModuleDefinition {
        memory: Some(ImportedMemory::max::<T>()),
        imported_functions: vec![ImportedFunction {
            module: "env",
            name: "gr_send",
            params: vec![
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
            ],
            return_type: Some(ValueType::I32),
        }],
        handle_body: Some(body::repeated_dyn(
            r,
            vec![
                Counter(dest_offset, offset_delta),
                Counter(payload_offset, offset_delta),
                ConstU32(payload_len),
                Counter(value_offset, offset_delta),
                ConstU32(10), // delay
                Counter(message_id_offset, offset_delta),
                Regular(Instruction::Call(0)),
                Regular(Instruction::Drop),
            ],
        )),
        ..Default::default()
    });
    let instance = Program::<T>::new(code, vec![])?;
    prepare::<T>(
        instance.caller.into_origin(),
        HandleKind::Handle(ProgramId::from_origin(instance.addr)),
        vec![],
        10000000u32.into(),
    )
}
