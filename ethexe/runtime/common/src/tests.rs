use crate::{process_dispatch, BlockConfig, RuntimeInterface, Storage, ProgramState, Dispatch, JournalHandler, RuntimeJournalHandler};
use core_processor::{
    common::{DispatchOutcome, JournalNote},
    configs::BlockInfo,
};
use gear_core::{
    code::{InstrumentedCode, CodeMetadata},
    ids::ActorId,
    message::{DispatchKind, MessageDetails},
    memory::{Memory, MemoryInterval, HostPointer},
    pages::{WasmPage, GearPage, WasmPagesAmount},
    costs::LazyPagesCosts,
};
use ethexe_common::gear::MessageType;
use gprimitives::{MessageId, H256};
use crate::state::{MemStorage, ActiveProgram, Program, MessageQueueHashWithSize};
use ethexe_common::MaybeHashOf;
use gear_core::program::MemoryInfix;
use gsys::{GasMultiplier, Percent};
use gear_core::gas_metering::Schedule;
use gear_core::code::MAX_WASM_PAGES_AMOUNT;
use core_processor::configs::SyscallName;
use alloc::collections::BTreeMap;
use gear_core::buffer::Payload;
use crate::state::PayloadLookup;
use gear_lazy_pages_common::{GlobalsAccessConfig, ProcessAccessError, Status};
use alloc::vec::Vec;

struct MockRuntimeInterface {
    storage: MemStorage,
}

impl RuntimeInterface<MemStorage> for MockRuntimeInterface {
    type LazyPages = MockLazyPages;

    fn block_info(&self) -> BlockInfo {
        BlockInfo::default()
    }
    fn init_lazy_pages(&self) {}
    fn random_data(&self) -> (Vec<u8>, u32) {
        (vec![], 0)
    }
    fn storage(&self) -> &MemStorage {
        &self.storage
    }
    fn update_state_hash(&self, _state_hash: &H256) {}
}

struct MockLazyPages;
impl gear_lazy_pages_common::LazyPagesInterface for MockLazyPages {
    fn try_to_enable_lazy_pages(_prefix: [u8; 32]) -> bool { true }

    fn init_for_program<Context>(
        _ctx: &mut Context,
        _mem: &mut impl Memory<Context>,
        _program_id: ActorId,
        _memory_infix: MemoryInfix,
        _stack_end: Option<WasmPage>,
        _globals_config: GlobalsAccessConfig,
        _costs: LazyPagesCosts,
    ) {}

    fn remove_lazy_pages_prot<Context>(_ctx: &mut Context, _mem: &mut impl Memory<Context>) {}

    fn update_lazy_pages_and_protect_again<Context>(
        _ctx: &mut Context,
        _mem: &mut impl Memory<Context>,
        _old_mem_addr: Option<HostPointer>,
        _old_mem_size: WasmPagesAmount,
        _new_mem_addr: HostPointer,
    ) {}

    fn get_write_accessed_pages() -> Vec<GearPage> { Vec::new() }

    fn get_status() -> Status { Status::Normal }

    fn pre_process_memory_accesses(
        _reads: &[MemoryInterval],
        _writes: &[MemoryInterval],
        _gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError> { Ok(()) }
}

#[test]
fn test_init_oog_does_not_terminate() {
    let storage = MemStorage::default();
    let ri = MockRuntimeInterface { storage };
    
    let program_id = ActorId::from(100);
    let mut program_state = ProgramState::zero();
    // Ensure program is active but uninitialized
    program_state.program = Program::Active(ActiveProgram {
        allocations_hash: MaybeHashOf::empty(),
        pages_hash: MaybeHashOf::empty(),
        memory_infix: MemoryInfix::new(0),
        initialized: false,
    });
    // Executable balance is 0 -> gas_limit will be 0

    let dispatch = Dispatch {
        id: MessageId::from(1),
        kind: DispatchKind::Init,
        source: ActorId::from(200),
        payload: PayloadLookup::Direct(Payload::new()), // empty payload
        value: 0,
        details: None,
        context: None,
        message_type: MessageType::Canonical,
        call: false,
    };

    let block_config = BlockConfig {
        block_info: BlockInfo::default(),
        forbidden_funcs: Default::default(),
        gas_multiplier: GasMultiplier::from_value_per_gas(100),
        costs: Schedule::default().process_costs(),
        max_pages: MAX_WASM_PAGES_AMOUNT.into(),
        outgoing_limit: 1024,
        outgoing_bytes_limit: 64 * 1024 * 1024,
        performance_multiplier: Percent::new(100),
        existential_deposit: 0,
        mailbox_threshold: 0,
        max_reservations: 0,
        reserve_for: 0,
    };

    let journal = process_dispatch(
        dispatch,
        &block_config,
        program_id,
        &program_state,
        &None, // No code loaded, but precharge checks gas first? 
               // Actually if code is None, it might fail differently.
               // But precharge happens before code loading in `process_dispatch`.
               // `context.charge_for_program` checks balance/gas first.
        &None,
        &ri,
        u64::MAX, // ample allowance for the queue processing itself
    );

    // Verify outcomes
    println!("Journal: {:?}", journal);
    let mut found = false;
    for note in &journal {
        if let JournalNote::MessageDispatched { outcome, .. } = note {
            match outcome {
                DispatchOutcome::NoExecution => {
                    found = true;
                }
                DispatchOutcome::InitFailure { reason, .. } => {
                    panic!("Should have been converted to NoExecution, but got InitFailure: {}", reason);
                }
                _ => {}
            }
        }
    }
    assert!(found, "Should have produced NoExecution outcome");

    // Also verify that handling this journal does NOT terminate the program
    let mut gas_allowance_counter = gear_core::gas::GasAllowanceCounter::new(10_000_000);
    let mut handler = RuntimeJournalHandler {
        storage: &ri.storage,
        program_state: &mut program_state,
        gas_allowance_counter: &mut gas_allowance_counter,
        gas_multiplier: &block_config.gas_multiplier,
        message_type: MessageType::Canonical,
        is_first_execution: true,
        stop_processing: false,
    };
    
    let _ = handler.handle_journal(journal);
    
    if let Program::Terminated(_) = program_state.program {
        panic!("Program should NOT be terminated");
    }
    if let Program::Active(active) = program_state.program {
        assert!(!active.initialized, "Program should remain uninitialized");
    }
}
