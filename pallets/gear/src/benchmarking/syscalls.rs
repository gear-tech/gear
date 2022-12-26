use super::code::{
    body::{self, DynInstr::*},
    max_pages, DataSegment, ImportedMemory, ModuleDefinition, WasmModule,
};
use crate::{
    manager::{CodeInfo, ExtManager, HandleKind},
    schedule::API_BENCHMARK_BATCH_SIZE,
    Config, CostsPerBlockOf, CurrencyOf, DbWeightOf, GasHandlerOf, MailboxOf, Pallet as Gear,
    QueueOf,
};
use codec::Encode;
use common::{
    benchmarking, scheduler::SchedulingCostsPerBlock, storage::*, CodeStorage, GasTree, Origin,
};
use core::{marker::PhantomData, mem, mem::size_of};
use core_processor::{
    configs::{AllocationsConfig, BlockConfig, BlockInfo},
    ContextChargedForCode, ContextChargedForInstrumentation,
};
use frame_support::traits::{Currency, Get};
use frame_system::RawOrigin;
use gear_core::{
    code::{Code, CodeAndId},
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::WasmPageNumber,
    message::{Dispatch, DispatchKind, Message, ReplyDetails, SignalDetails},
    reservation::GasReservationSlot,
};
use gear_wasm_instrument::{parity_wasm::elements::Instruction, syscalls::SysCallName};
use sp_core::H256;
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{convert::TryInto, prelude::*};

use super::{Exec, Program};

fn prepare<T>(
    source: H256,
    kind: HandleKind,
    payload: Vec<u8>,
    value: u128,
    err_len_ptrs: Vec<u32>,
) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    #[cfg(feature = "lazy-pages")]
    assert!(gear_lazy_pages_common::try_to_enable_lazy_pages());

    // to see logs in bench tests
    let _ = env_logger::try_init();

    let ext_manager = ExtManager::<T>::default();
    let bn: u64 = Gear::<T>::block_number().unique_saturated_into();
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
                    Some(ReplyDetails::new(msg.id(), exit_code).into()),
                ),
            )
        }
        HandleKind::Signal(msg_id, status_code) => {
            let (msg, _bn) =
                MailboxOf::<T>::remove(<T::AccountId as Origin>::from_origin(source), msg_id)
                    .map_err(|_| "Internal error: unable to find message in mailbox")?;
            Dispatch::new(
                DispatchKind::Signal,
                Message::new(
                    root_message_id,
                    ProgramId::from_origin(source),
                    msg.source(),
                    payload.try_into()?,
                    Some(u64::MAX),
                    value,
                    Some(SignalDetails::new(msg.id(), status_code).into()),
                ),
            )
        }
    };

    let initial_gas = u64::MAX;
    let origin = <T::AccountId as Origin>::from_origin(source);
    GasHandlerOf::<T>::create(origin, root_message_id, initial_gas)
        .map_err(|_| "Internal error: unable to create gas handler")?;

    let dispatch = dispatch.into_stored();

    QueueOf::<T>::clear();

    QueueOf::<T>::queue(dispatch).map_err(|_| "Messages storage corrupted")?;

    let block_info = BlockInfo {
        height: Gear::<T>::block_number().unique_saturated_into(),
        timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
    };

    let existential_deposit = CurrencyOf::<T>::minimum_balance().unique_saturated_into();
    let mailbox_threshold = <T as Config>::MailboxThreshold::get();
    let waitlist_cost = CostsPerBlockOf::<T>::waitlist();
    let reserve_for = CostsPerBlockOf::<T>::reserve_for().unique_saturated_into();
    let reservation = CostsPerBlockOf::<T>::reservation().unique_saturated_into();

    let schedule = T::Schedule::get();
    let block_config = BlockConfig {
        block_info,
        allocations_config: AllocationsConfig {
            max_pages: T::Schedule::get().limits.memory_pages.into(),
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
        max_reservations: u64::MAX,
        code_instrumentation_cost: schedule.code_instrumentation_cost,
        code_instrumentation_byte_cost: schedule.code_instrumentation_byte_cost,
    };

    let queued_dispatch = match QueueOf::<T>::dequeue().map_err(|_| "MQ storage corrupted")? {
        Some(d) => d,
        None => return Err("Dispatch not found"),
    };

    let actor_id = queued_dispatch.destination();
    let actor = ext_manager
        .get_actor(actor_id)
        .ok_or("Program not found in the storage")?;

    let precharged_dispatch = core_processor::precharge_for_program(
        &block_config,
        u64::MAX,
        queued_dispatch.into_incoming(initial_gas),
        actor_id,
    )
    .map_err(|_| "core_processor::precharge_for_program failed")?;

    let balance = actor.balance;
    let context = core_processor::precharge_for_code_length(
        &block_config,
        precharged_dispatch,
        actor_id,
        actor.executable_data,
    )
    .map_err(|_| "core_processor::precharge_for_code failed")?;

    let code =
        T::CodeStorage::get_code(context.actor_data().code_id).ok_or("Program code not found")?;

    let context = ContextChargedForCode::from((context, code.code().len() as u32));
    let context = core_processor::precharge_for_memory(
        &block_config,
        ContextChargedForInstrumentation::from(context),
        false,
    )
    .map_err(|_| "core_processor::precharge_for_memory failed")?;

    let origin = ProgramId::from_origin(source);

    Ok(Exec {
        ext_manager,
        block_config,
        context: (context, code, balance, origin).into(),
        random_data: (vec![0u8; 32], 0),
        // actor without pages data because of lazy pages enabled
        memory_pages: Default::default(),
        err_len_ptrs,
    })
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
    fn prepare_handle(
        code: WasmModule<T>,
        value: u32,
        err_len_ptrs: Vec<u32>,
    ) -> Result<Exec<T>, &'static str> {
        let instance = Program::<T>::new(code, vec![])?;

        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            value.into(),
            err_len_ptrs,
        )
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
        Self::prepare_handle(code, 0, vec![])
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
        Self::prepare_handle(code, 0, vec![])
    }

    pub fn gr_reserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
        let err_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .map(|x| x + 1) // +1 is for unaligned read
            .collect();

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReserveGas],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // gas amount
                    Regular(Instruction::I64Const(1)),
                    // duration
                    Regular(Instruction::I32Const(1)),
                    // err_rid ptr
                    Counter(1, 4),
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

        let amount_offset = reservation_id_offset + reservation_id_bytes.len() as u32;

        let err_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .map(|x| x + amount_offset)
            .collect();

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
                    Counter(amount_offset, 4),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        // insert gas reservation slots
        let mut program = common::get_active_program(instance.addr).unwrap();
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
        common::set_program(instance.addr, program);

        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            0,
            err_ptrs,
        )
    }

    pub fn gr_system_reserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
        let err_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .map(|x| x + 1) // +1 is for unaligned read
            .collect();

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::SystemReserveGas],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // gas amount
                    Regular(Instruction::I64Const(50_000_000)),
                    // err len ptr
                    Counter(1, 4),
                    // CALL
                    Regular(Instruction::Call(0)),
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
            err_ptrs,
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
        Self::prepare_handle(code, 0, vec![])
    }

    pub fn gr_read(r: u32) -> Result<Exec<T>, &'static str> {
        let buffer_offset = 1;
        let buffer_len = 100u32;

        let err_len_offset = buffer_offset + buffer_len;
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_len_offset)
            .collect();

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
                    Counter(err_len_offset, 4),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![0; (buffer_len + buffer_offset) as usize],
            0,
            err_len_ptrs,
        )
    }

    pub fn gr_read_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let buffer_offset = 1;
        let buffer = vec![0xff; (n * 1024) as usize];
        let buffer_len = buffer.len() as u32;

        let err_len_offset = buffer_offset + buffer_len;
        let err_len_ptrs = (0..API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_len_offset)
            .collect();

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Read],
            data_segments: vec![DataSegment {
                offset: buffer_offset,
                value: buffer,
            }],
            handle_body: Some(body::repeated_dyn(
                API_BENCHMARK_BATCH_SIZE,
                vec![
                    // at
                    Regular(Instruction::I32Const(0)),
                    // len
                    Regular(Instruction::I32Const(buffer_len as i32)),
                    // buffer ptr
                    Regular(Instruction::I32Const(buffer_offset as i32)),
                    // err len ptr
                    Counter(err_len_offset, 4),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![0; (buffer_len + buffer_len) as usize],
            0,
            err_len_ptrs,
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
        Self::prepare_handle(code, 0, vec![])
    }

    pub fn gr_send_init(r: u32) -> Result<Exec<T>, &'static str> {
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + 1) // +1 is for unaligned read
            .collect();

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::SendInit],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_handle ptr
                    Counter(1, 4),
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
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_offset)
            .collect();

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
                    Counter(err_offset, 4),
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
        let err_len_ptrs = (0..API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_offset)
            .collect();

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
                    Counter(err_offset, 4),
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
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_mid_offset)
            .collect();

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
                    Counter(err_mid_offset, 4),
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
        let err_len_ptrs = (0..API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_mid_offset)
            .collect();

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
                    Counter(err_mid_offset, 4),
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
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_mid_offset)
            .collect();

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
                    Counter(err_mid_offset, 4),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        // insert gas reservation slots
        let mut program = common::get_active_program(instance.addr).unwrap();
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
        common::set_program(instance.addr, program);

        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            0,
            err_len_ptrs,
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
        let err_len_ptrs = (0..API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_mid_offset)
            .collect();

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
                    Counter(err_mid_offset, 4),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        // insert gas reservation slots
        let mut program = common::get_active_program(instance.addr).unwrap();
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
        common::set_program(instance.addr, program);

        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            0,
            err_len_ptrs,
        )
    }

    pub fn gr_reply_commit(r: u32) -> Result<Exec<T>, &'static str> {
        let value_offset = 1;

        let err_mid_offset = value_offset + mem::size_of::<u128>() as u32;
        let err_len_ptrs = (0..r / 4).step_by(4).map(|x| x + err_mid_offset).collect();

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
                    Counter(err_mid_offset, 4),
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
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_offset)
            .collect();

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
                    Counter(err_offset, 4),
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
        Self::prepare_handle(code, 10000000, vec![err_offset as u32])
    }

    pub fn gr_reservation_reply_commit(r: u32) -> Result<Exec<T>, &'static str> {
        let rid_value_offset = 1;

        let rid_values = (0..r * API_BENCHMARK_BATCH_SIZE)
            .flat_map(|i| {
                let mut bytes = [0; 32 + 16];
                bytes[..32].copy_from_slice(ReservationId::from(i as u64).as_ref());
                bytes.to_vec()
            })
            .collect::<Vec<_>>();

        let err_mid_offset = rid_value_offset + rid_values.len() as u32;
        let err_len_ptrs = (0..r / 4).map(|x| x + err_mid_offset).collect();

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
                    Counter(err_mid_offset, 4),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        // insert gas reservation slots
        let mut program = common::get_active_program(instance.addr).unwrap();
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
        common::set_program(instance.addr, program);

        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            0,
            err_len_ptrs,
        )
    }

    pub fn gr_reply_to(r: u32) -> Result<Exec<T>, &'static str> {
        let err_mid_ptr = 1;
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_mid_ptr)
            .collect();

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::ReplyTo],
            reply_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_mid ptr
                    Counter(err_mid_ptr, 4),
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
        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Reply(msg_id, 0),
            vec![],
            0u32.into(),
            err_len_ptrs,
        )
    }

    pub fn gr_signal_from(r: u32) -> Result<Exec<T>, &'static str> {
        let err_mid_ptr = 1;
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_mid_ptr)
            .collect();

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::SignalFrom],
            reply_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_mid ptr
                    Counter(err_mid_ptr, 4),
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
        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Signal(msg_id, 1),
            vec![],
            0u32.into(),
            err_len_ptrs,
        )
    }

    pub fn gr_reply_push_input(r: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = 100;

        let err_offset = payload_offset + payload_len;
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_offset)
            .collect();

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
                    Counter(err_offset, 4),
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;
        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![1u8; payload_len as usize],
            0u32.into(),
            err_len_ptrs,
        )
    }

    pub fn gr_reply_push_input_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = n as i32 * 1_024;

        let err_offset = payload_offset + payload_len;

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
        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![1u8; payload_len as usize],
            0u32.into(),
            vec![err_offset as u32],
        )
    }

    pub fn gr_send_push_input(r: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = 100u32;

        let err_handle_ptr = payload_offset + payload_len;

        let err_offset = err_handle_ptr + 8;
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_offset)
            .collect();

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
                    Counter(err_offset, 4),
                    // CALL push
                    Regular(Instruction::Call(1)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;
        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![1u8; payload_len as usize],
            0u32.into(),
            err_len_ptrs,
        )
    }

    pub fn gr_send_push_input_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = n * 1_024;

        let err_handle_ptr = payload_offset + payload_len;

        let err_offset = err_handle_ptr + 8;
        let err_len_ptrs = (0..API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_offset)
            .collect();

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
                    Counter(err_offset, 4),
                    // CALL push
                    Regular(Instruction::Call(1)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;
        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![1u8; payload_len as usize],
            0u32.into(),
            err_len_ptrs,
        )
    }

    pub fn gr_status_code(r: u32) -> Result<Exec<T>, &'static str> {
        let err_code_ptr = 1;
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_code_ptr)
            .collect();

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::StatusCode],
            reply_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    // err_code ptr
                    Counter(err_code_ptr, 4),
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
        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Reply(msg_id, 0),
            vec![],
            0u32.into(),
            err_len_ptrs,
        )
    }

    pub fn gr_debug(r: u32) -> Result<Exec<T>, &'static str> {
        let string_offset = 1;
        let string_len = 100;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Debug],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    // payload ptr
                    Instruction::I32Const(string_offset),
                    // payload len
                    Instruction::I32Const(string_len),
                    // CALL
                    Instruction::Call(0),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, vec![])
    }

    pub fn gr_debug_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let string_offset = 1;
        let string_len = n as i32 * 1024;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![SysCallName::Debug],
            handle_body: Some(body::repeated(
                API_BENCHMARK_BATCH_SIZE,
                &[
                    // payload ptr
                    Instruction::I32Const(string_offset),
                    // payload len
                    Instruction::I32Const(string_len),
                    // CALL
                    Instruction::Call(0),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, vec![])
    }

    pub fn gr_error(r: u32) -> Result<Exec<T>, &'static str> {
        let status_code_offset = 1;
        let error_len_offset = status_code_offset + size_of::<i32>() as u32;
        let error_offset = error_len_offset + size_of::<u32>() as u32;
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + error_offset)
            .collect();

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
                    // error ptr
                    Counter(error_offset, 4),
                    // error length ptr
                    Regular(Instruction::I32Const(error_len_offset as i32)),
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
        Self::prepare_handle(code, 0, vec![])
    }

    pub fn gr_wake(r: u32) -> Result<Exec<T>, &'static str> {
        let message_id_offset = 1;

        let message_ids = (0..r * API_BENCHMARK_BATCH_SIZE)
            .flat_map(|i| <[u8; 32]>::from(MessageId::from(i as u64)).to_vec())
            .collect::<Vec<_>>();

        let err_offset = message_id_offset + message_ids.len() as u32;
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_offset)
            .collect();

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
                    Counter(err_offset, 4),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;

        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            0,
            err_len_ptrs,
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
        let err_len_ptrs = (0..r * API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_mid_pid_offset)
            .collect();

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
                    Counter(err_mid_pid_offset, 4),
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
                    Counter(err_mid_pid_offset, 4),
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
        let err_len_ptrs = (0..API_BENCHMARK_BATCH_SIZE / 4)
            .step_by(4)
            .map(|x| x + err_mid_pid_offset)
            .collect();

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
                    Counter(err_mid_pid_offset, 4),
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
                    Counter(err_mid_pid_offset, 4),
                    // CALL
                    Regular(Instruction::Call(0)),
                ],
            )),
            ..Default::default()
        });

        Self::prepare_handle(code, 0, err_len_ptrs)
    }

    pub fn lazy_pages_read_access(wasm_pages: WasmPageNumber) -> Result<Exec<T>, &'static str> {
        let instrs = body::read_access_all_pages_instrs(wasm_pages, vec![]);
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, vec![])
    }

    pub fn lazy_pages_write_access(wasm_pages: WasmPageNumber) -> Result<Exec<T>, &'static str> {
        let mut instrs = body::read_access_all_pages_instrs(max_pages::<T>().into(), vec![]);
        instrs = body::write_access_all_pages_instrs(wasm_pages, instrs);
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            handle_body: Some(body::from_instructions(instrs)),
            ..Default::default()
        });
        Self::prepare_handle(code, 0, vec![])
    }
}
