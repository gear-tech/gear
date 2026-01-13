// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
    BlockConfig, Dispatch, JournalHandler, ProgramState, RuntimeInterface, RuntimeJournalHandler,
    Storage, process_dispatch,
    state::{ActiveProgram, MemStorage, PayloadLookup, Program},
};
use alloc::vec::Vec;
use core_processor::{
    common::{DispatchOutcome, InitFailureReason, JournalNote},
    configs::BlockInfo,
};
use ethexe_common::{MaybeHashOf, gear::MessageType};
use gear_core::{
    buffer::Payload,
    code::MAX_WASM_PAGES_AMOUNT,
    costs::LazyPagesCosts,
    gas_metering::Schedule,
    ids::ActorId,
    memory::{HostPointer, Memory, MemoryInterval},
    message::DispatchKind,
    pages::{GearPage, WasmPage, WasmPagesAmount},
    program::MemoryInfix,
};
use gear_lazy_pages_common::{GlobalsAccessConfig, ProcessAccessError, Status};
use gprimitives::{H256, MessageId};
use gsys::{GasMultiplier, Percent};

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
    fn try_to_enable_lazy_pages(_prefix: [u8; 32]) -> bool {
        true
    }

    fn init_for_program<Context>(
        _ctx: &mut Context,
        _mem: &mut impl Memory<Context>,
        _program_id: ActorId,
        _memory_infix: MemoryInfix,
        _stack_end: Option<WasmPage>,
        _globals_config: GlobalsAccessConfig,
        _costs: LazyPagesCosts,
    ) {
    }

    fn remove_lazy_pages_prot<Context>(_ctx: &mut Context, _mem: &mut impl Memory<Context>) {}

    fn update_lazy_pages_and_protect_again<Context>(
        _ctx: &mut Context,
        _mem: &mut impl Memory<Context>,
        _old_mem_addr: Option<HostPointer>,
        _old_mem_size: WasmPagesAmount,
        _new_mem_addr: HostPointer,
    ) {
    }

    fn get_write_accessed_pages() -> Vec<GearPage> {
        Vec::new()
    }

    fn get_status() -> Status {
        Status::Normal
    }

    fn pre_process_memory_accesses(
        _reads: &[MemoryInterval],
        _writes: &[MemoryInterval],
        _gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError> {
        Ok(())
    }
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
        &None,
        // Precharge happens before code loading, so missing code doesn't affect this OOG test.
        &None,
        &ri,
        u64::MAX,
    );

    // Verify outcomes
    println!("Journal: {:?}", journal);
    let mut found = false;
    for note in &journal {
        if let JournalNote::MessageDispatched { outcome, .. } = note {
            if let DispatchOutcome::InitFailure { reason, .. } = outcome {
                assert_eq!(*reason, InitFailureReason::RanOutOfGas);
                found = true;
            }
        }
    }
    assert!(found, "Should have produced InitFailure with RanOutOfGas");

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

#[test]
fn test_init_oog_after_first_precharge_does_not_terminate() {
    let storage = MemStorage::default();
    let ri = MockRuntimeInterface { storage };

    let program_id = ActorId::from(101);
    let mut program_state = ProgramState::zero();
    program_state.program = Program::Active(ActiveProgram {
        allocations_hash: MaybeHashOf::empty(),
        pages_hash: MaybeHashOf::empty(),
        memory_infix: MemoryInfix::new(0),
        initialized: false,
    });

    let dispatch = Dispatch {
        id: MessageId::from(2),
        kind: DispatchKind::Init,
        source: ActorId::from(201),
        payload: PayloadLookup::Direct(Payload::new()),
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

    let first_charge = block_config.costs.db.read.cost_for_one();
    let gas_limit = first_charge + 1;
    program_state.executable_balance = block_config.gas_multiplier.gas_to_value(gas_limit);

    let journal = process_dispatch(
        dispatch,
        &block_config,
        program_id,
        &program_state,
        &None,
        &None,
        &ri,
        u64::MAX,
    );

    let mut found = false;
    for note in &journal {
        if let JournalNote::MessageDispatched { outcome, .. } = note {
            if let DispatchOutcome::InitFailure { reason, .. } = outcome {
                assert_eq!(*reason, InitFailureReason::RanOutOfGas);
                found = true;
            }
        }
    }
    assert!(found, "Should have produced InitFailure with RanOutOfGas");

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

#[test]
fn test_init_allowance_exceed_does_not_terminate() {
    let storage = MemStorage::default();
    let ri = MockRuntimeInterface { storage };

    let program_id = ActorId::from(102);
    let mut program_state = ProgramState::zero();
    program_state.program = Program::Active(ActiveProgram {
        allocations_hash: MaybeHashOf::empty(),
        pages_hash: MaybeHashOf::empty(),
        memory_infix: MemoryInfix::new(0),
        initialized: false,
    });

    let dispatch = Dispatch {
        id: MessageId::from(3),
        kind: DispatchKind::Init,
        source: ActorId::from(202),
        payload: PayloadLookup::Direct(Payload::new()),
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

    let min_gas = block_config.costs.db.read.cost_for_one();
    program_state.executable_balance = block_config.gas_multiplier.gas_to_value(min_gas);

    let journal = process_dispatch(
        dispatch,
        &block_config,
        program_id,
        &program_state,
        &None,
        &None,
        &ri,
        // no block allowance
        0,
    );

    let mut found = false;
    for note in &journal {
        if let JournalNote::MessageDispatched { outcome, .. } = note {
            if let DispatchOutcome::InitFailure { reason, .. } = outcome {
                assert_eq!(*reason, InitFailureReason::RanOutOfAllowance);
                found = true;
            }
        }
    }
    assert!(
        found,
        "Should have produced InitFailure with RanOutOfAllowance"
    );

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
