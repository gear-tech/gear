use super::code::{
    body::{self, DynInstr::*},
    max_pages, DataSegment, ImportedMemory, ModuleDefinition, WasmModule,
};
use crate::{
    manager::{CodeInfo, ExtManager, HandleKind},
    schedule::API_BENCHMARK_BATCH_SIZE,
    BlockGasLimitOf, Config, CostsPerBlockOf, CurrencyOf, DbWeightOf, GasHandlerOf, MailboxOf,
    Pallet as Gear, QueueOf, WaitlistOf,
};
use codec::Encode;
use common::{
    benchmarking, scheduler::SchedulingCostsPerBlock, storage::*, CodeStorage, GasTree, Origin,
};
use core::{marker::PhantomData, mem::size_of};
use core_processor::{
    configs::{AllocationsConfig, BlockConfig, BlockInfo, MessageExecutionContext},
    PrechargeResult, PrepareResult,
};
use frame_support::traits::{Currency, Get};
use frame_system::RawOrigin;
use gear_core::{
    code::{Code, CodeAndId},
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    message::{Dispatch, DispatchKind, Message, ReplyDetails},
    reservation::GasReservationSlot,
};
use gear_wasm_instrument::{parity_wasm::elements::Instruction, syscalls::syscall_signature};
use sp_core::H256;
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{convert::TryInto, prelude::*};

use super::{Exec, Program};

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
    #[cfg(feature = "lazy-pages")]
    assert!(gear_lazy_pages_common::try_to_enable_lazy_pages());

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
            random_data: (vec![0u8; 32], 0),
            // actor without pages data because of lazy pages enabled
            memory_pages: Default::default(),
        })
    } else {
        Err("Dispatch not found")
    }
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
    fn prepare_handle(code: WasmModule<T>, value: u32) -> Result<Exec<T>, &'static str> {
        let instance = Program::<T>::new(code, vec![])?;
        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            value.into(),
        )
    }

    pub fn alloc(r: u32) -> Result<Exec<T>, &'static str> {
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory { min_pages: 0 }),
            imported_functions: vec!["alloc"],
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
        Self::prepare_handle(code, 0)
    }

    pub fn free(r: u32) -> Result<Exec<T>, &'static str> {
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
            imported_functions: vec!["alloc", "free"],
            init_body: None,
            handle_body: Some(body::plain(instructions)),
            ..Default::default()
        });
        Self::prepare_handle(code, 0)
    }

    pub fn gr_reserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
        let id_offset = 1;
        let id_bytes = u128::MAX.encode();

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_reserve_gas"],
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
        Self::prepare_handle(code, 0)
    }

    // TODO: currently each syscall execution returns error: ExecutionError::InvalidReservationId.
    // We need to fill reservations set with data first. (issue #1724)
    pub fn gr_unreserve_gas(r: u32) -> Result<Exec<T>, &'static str> {
        let id_bytes = u128::MAX.encode();
        let id_len = id_bytes.len() as u32;
        let id_offset = 1;
        let amount_bytes = 1000u64.encode();
        let amount_offset = id_offset + id_len;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_unreserve_gas"],
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
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(id_offset as i32),     // id ptr
                    Instruction::I32Const(amount_offset as i32), // unreserved amount ptr
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0)
    }

    pub fn getter(name: &'static str, r: u32) -> Result<Exec<T>, &'static str> {
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![name],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[Instruction::I32Const(0), Instruction::Call(0)],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0)
    }

    pub fn number_getter(name: &'static str, r: u32) -> Result<Exec<T>, &'static str> {
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![name],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[Instruction::Call(0), Instruction::Drop],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0)
    }

    pub fn gr_read(r: u32) -> Result<Exec<T>, &'static str> {
        let buffer_offset = 1;
        let payload = vec![1u8; 100];

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_read"],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(0),
                    Instruction::I32Const(payload.len() as i32),
                    Instruction::I32Const(buffer_offset),
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0)
    }

    pub fn gr_read_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let buffer_offset = 1;
        let payload = vec![0xff; (n * 1024) as usize];

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_read"],
            handle_body: Some(body::repeated(
                API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(0),
                    Instruction::I32Const(payload.len() as i32),
                    Instruction::I32Const(buffer_offset),
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0)
    }

    pub fn gr_random(r: u32) -> Result<Exec<T>, &'static str> {
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_random"],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(0),  // subject ptr
                    Instruction::I32Const(32), // subject len
                    Instruction::I32Const(33), // random ptr
                    Instruction::I32Const(0),  // bn ptr
                    Instruction::Call(0),
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

    pub fn gr_send_init(r: u32) -> Result<Exec<T>, &'static str> {
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_send_init"],
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
        Self::prepare_handle(code, 0)
    }

    pub fn gr_send_push(r: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = 100;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_send_init", "gr_send_push"],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(0), // handle
                    Instruction::Call(0),
                    Instruction::Drop,
                    Instruction::I32Const(0), // handle
                    Instruction::I32Const(payload_offset),
                    Instruction::I32Const(payload_len),
                    Instruction::Call(1),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0)
    }

    // TODO: investigate how handle changes can affect on syscall perf (issue #1722).
    pub fn gr_send_push_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = n * 1024;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_send_init", "gr_send_push"],
            handle_body: Some(body::repeated(
                API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(0), // handle
                    Instruction::Call(0),
                    Instruction::Drop,
                    Instruction::I32Const(0), // handle
                    Instruction::I32Const(payload_offset),
                    Instruction::I32Const(payload_len as i32),
                    Instruction::Call(1),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0)
    }

    // Benchmark the `gr_send_commit` call.
    // `gr_send` call is shortcut for `gr_send_init` + `gr_send_commit`
    pub fn gr_send_commit(r: u32) -> Result<Exec<T>, &'static str> {
        let offset = 1;
        let payload_len = 100;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_send"],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(offset), // dest ptr
                    Instruction::I32Const(offset), // payload ptr
                    Instruction::I32Const(payload_len),
                    Instruction::I32Const(offset), // value ptr
                    Instruction::I32Const(10),     // delay
                    Instruction::I32Const(offset), // message_id ptr
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 10000000)
    }

    // Benchmark the `gr_send_commit` call.
    // `gr_send` call is shortcut for `gr_send_init` + `gr_send_commit`
    pub fn gr_send_commit_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let offset = 1;
        let payload_len = n as i32 * 1024;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_send"],
            handle_body: Some(body::repeated(
                API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(offset), // dest ptr
                    Instruction::I32Const(offset), // payload ptr
                    Instruction::I32Const(payload_len),
                    Instruction::I32Const(offset), // value ptr
                    Instruction::I32Const(10),     // delay
                    Instruction::I32Const(offset), // message_id ptr
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 10000000)
    }

    pub fn gr_reply_commit(r: u32) -> Result<Exec<T>, &'static str> {
        let offset = 1;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_reply_commit"],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(offset), // value ptr
                    Instruction::I32Const(10),     // delay
                    Instruction::I32Const(offset), // result: message_id ptr
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 10000000)
    }

    pub fn gr_reply_push(r: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = 100;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_reply_push"],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(payload_offset),
                    Instruction::I32Const(payload_len),
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 10000000)
    }

    pub fn gr_reply_push_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let payload_offset = 1;
        let payload_len = n as i32 * 1024;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_reply_push"],
            handle_body: Some(body::repeated(
                API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(payload_offset),
                    Instruction::I32Const(payload_len),
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 10000000)
    }

    pub fn gr_reply_to(r: u32) -> Result<Exec<T>, &'static str> {
        let message_id_offset = 1;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_reply_to"],
            reply_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(message_id_offset),
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let msg_id = MessageId::from(10);
        let msg = gear_core::message::Message::new(
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
        )
    }

    pub fn gr_exit_code(r: u32) -> Result<Exec<T>, &'static str> {
        let exit_code_offset = 1;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_exit_code"],
            reply_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(exit_code_offset),
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        let instance = Program::<T>::new(code, vec![])?;
        let msg_id = MessageId::from(10);
        let msg = gear_core::message::Message::new(
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
        )
    }

    pub fn gr_debug(r: u32) -> Result<Exec<T>, &'static str> {
        let string_offset = 1;
        let string_len = 100;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_debug"],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(string_offset),
                    Instruction::I32Const(string_len),
                    Instruction::Call(0),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0)
    }

    pub fn gr_debug_per_kb(n: u32) -> Result<Exec<T>, &'static str> {
        let string_offset = 1;
        let string_len = n as i32 * 1024;

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_debug"],
            handle_body: Some(body::repeated(
                API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(string_offset),
                    Instruction::I32Const(string_len),
                    Instruction::Call(0),
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0)
    }

    pub fn no_return_bench(
        name: &'static str,
        param: Option<u32>,
        r: u32,
    ) -> Result<Exec<T>, &'static str> {
        assert!(r <= 1);

        let instructions = if let Some(c) = param {
            assert!(syscall_signature(name).params.len() == 1);
            vec![Instruction::I32Const(c as i32), Instruction::Call(0)]
        } else {
            assert!(syscall_signature(name).params.is_empty());
            vec![Instruction::Call(0)]
        };

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec![name],
            handle_body: Some(body::repeated(r, &instructions)),
            ..Default::default()
        });
        Self::prepare_handle(code, 0)
    }

    pub fn gr_wake(r: u32) -> Result<Exec<T>, &'static str> {
        let offset = 1;
        let message_ids = (0..r)
            .map(|i| MessageId::from(i as u64))
            .collect::<Vec<_>>();
        let message_id_bytes = message_ids.iter().flat_map(|x| x.encode()).collect();

        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_wake"],
            data_segments: vec![DataSegment {
                offset,
                value: message_id_bytes,
            }],
            handle_body: Some(body::repeated_dyn(
                r * API_BENCHMARK_BATCH_SIZE,
                vec![
                    Counter(offset, size_of::<MessageId>() as u32), // message_id ptr
                    Regular(Instruction::I32Const(10)),             // delay
                    Regular(Instruction::Call(0)),
                    Regular(Instruction::Drop),
                ],
            )),
            ..Default::default()
        });

        let instance = Program::<T>::new(code, vec![])?;
        for message_id in message_ids {
            let message = gear_core::message::Message::new(
                message_id,
                1.into(),
                ProgramId::from(instance.addr.as_bytes()),
                Default::default(),
                Some(1_000_000),
                0,
                None,
            );
            let dispatch = gear_core::message::Dispatch::new(
                gear_core::message::DispatchKind::Handle,
                message,
            )
            .into_stored();
            WaitlistOf::<T>::insert(dispatch.clone(), u32::MAX.unique_saturated_into())
                .expect("Duplicate wl message");
        }
        prepare::<T>(
            instance.caller.into_origin(),
            HandleKind::Handle(ProgramId::from_origin(instance.addr)),
            vec![],
            0u32.into(),
        )
    }

    pub fn gr_create_program_wgas(r: u32) -> Result<Exec<T>, &'static str> {
        let module = WasmModule::<T>::dummy();
        let code_hash_bytes = module.hash.encode();
        let code_hash_len = code_hash_bytes.len();
        let code_hash_offset = 1;

        let salt_bytes = u8::MAX.encode();
        let salt_bytes_len = salt_bytes.len();
        let salt_offset = code_hash_offset + code_hash_len as u32;

        let value_bytes = u128::MAX.encode();
        let value_bytes_len = value_bytes.len();
        let value_offset = salt_offset + salt_bytes_len as u32;

        let payload = vec![1, 2, 3];
        let payload_len = payload.len();
        let payload_offset = value_offset + value_bytes_len as u32;

        let message_id_offset = 1;
        let program_id_offset = 1;

        let _ = Gear::<T>::upload_code_raw(
            RawOrigin::Signed(benchmarking::account("instantiator", 0, 0)).into(),
            module.code,
        );
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_create_program_wgas"],
            data_segments: vec![
                DataSegment {
                    offset: code_hash_offset,
                    value: code_hash_bytes,
                },
                DataSegment {
                    offset: salt_offset,
                    value: salt_bytes,
                },
                DataSegment {
                    offset: value_offset,
                    value: value_bytes,
                },
                DataSegment {
                    offset: payload_offset,
                    value: payload,
                },
            ],
            handle_body: Some(body::repeated(
                r * API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(code_hash_offset as i32), // code_id ptr
                    Instruction::I32Const(salt_offset as i32),      // salt ptr
                    Instruction::I32Const(salt_bytes_len as i32),   // salt len
                    Instruction::I32Const(payload_offset as i32),   // payload ptr
                    Instruction::I32Const(payload_len as i32),      // payload len
                    Instruction::I64Const(100000000),               // gas limit
                    Instruction::I32Const(value_offset as i32),     // value ptr
                    Instruction::I32Const(10),                      // delay
                    Instruction::I32Const(message_id_offset),       // message_id ptr
                    Instruction::I32Const(program_id_offset),       // program_id ptr
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });
        Self::prepare_handle(code, 0)
    }

    pub fn gr_create_program_wgas_per_kb(pkb: u32, skb: u32) -> Result<Exec<T>, &'static str> {
        let module = WasmModule::<T>::dummy();
        let code_hash_bytes = module.hash.encode();
        let code_hash_len = code_hash_bytes.len();
        let code_hash_offset = 1;

        let value_bytes = u128::MAX.encode();
        let value_offset = code_hash_offset + code_hash_len as u32;

        let salt_bytes_len = skb * 1024;
        let payload_len = pkb * 1024;
        let payload_offset = 1;
        let salt_offset = 1;
        let message_id_offset = 1;
        let program_id_offset = 1;

        let _ = Gear::<T>::upload_code_raw(
            RawOrigin::Signed(benchmarking::account("instantiator", 0, 0)).into(),
            module.code,
        );
        let code = WasmModule::<T>::from(ModuleDefinition {
            memory: Some(ImportedMemory::max::<T>()),
            imported_functions: vec!["gr_create_program_wgas"],
            data_segments: vec![
                DataSegment {
                    offset: code_hash_offset,
                    value: code_hash_bytes,
                },
                DataSegment {
                    offset: value_offset,
                    value: value_bytes,
                },
            ],
            handle_body: Some(body::repeated(
                API_BENCHMARK_BATCH_SIZE,
                &[
                    Instruction::I32Const(code_hash_offset as i32), // code_hash ptr
                    Instruction::I32Const(salt_offset),             // salt ptr
                    Instruction::I32Const(salt_bytes_len as i32),   // salt len
                    Instruction::I32Const(payload_offset),          // payload ptr
                    Instruction::I32Const(payload_len as i32),      // payload len
                    Instruction::I64Const(100000000),               // gas limit
                    Instruction::I32Const(value_offset as i32),     // value ptr
                    Instruction::I32Const(10),                      // delay
                    Instruction::I32Const(message_id_offset),       // message_id ptr
                    Instruction::I32Const(program_id_offset),       // program_id ptr
                    Instruction::Call(0),
                    Instruction::Drop,
                ],
            )),
            ..Default::default()
        });

        Self::prepare_handle(code, 0)
    }
}
