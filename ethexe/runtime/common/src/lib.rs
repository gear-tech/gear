// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Runtime common implementation.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use core_processor::{
    CachedExecutionData, ContextCharged, ExecutionStep, Ext, PrechargeContext,
    ProcessorExternalities, SequenceState,
    common::{ExecutableActorData, JournalNote, Program, ReservationsAndMemorySize},
    configs::{BlockConfig, ExecutionSettings, SyscallName},
    execute_wasm_step,
};
use ethexe_common::gear::{CHUNK_PROCESSING_GAS_LIMIT, MessageType};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode, MAX_WASM_PAGES_AMOUNT},
    gas::GasAllowanceCounter,
    gas_metering::Schedule,
    ids::ActorId,
    message::{ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage},
    pages::{WasmPage, numerated::tree::IntervalsTree},
};
use gear_core_backend::MemorySnapshot;
use gear_lazy_pages_common::LazyPagesInterface;
use gprimitives::H256;
use gsys::{GasMultiplier, Percent};
use journal::RuntimeJournalHandler;
use state::{Dispatch, ProgramState, Storage};

pub use core_processor::configs::BlockInfo;

pub use journal::NativeJournalHandler as JournalHandler;
pub use schedule::{Handler as ScheduleHandler, Restorer as ScheduleRestorer};
pub use transitions::{FinalizedBlockTransitions, InBlockTransitions, NonFinalTransition};

pub mod state;

mod journal;
mod schedule;
mod transitions;

#[cfg(test)]
mod tests;

// TODO: consider format.
/// Version of the runtime.
pub const VERSION: u32 = 1;
pub const RUNTIME_ID: u32 = 1;

pub type ProgramJournals = Vec<(Vec<JournalNote>, MessageType, bool)>;

pub trait RuntimeInterface<S: Storage> {
    type LazyPages: LazyPagesInterface + 'static;

    fn block_info(&self) -> BlockInfo;
    fn init_lazy_pages(&self);
    fn random_data(&self) -> (Vec<u8>, u32);
    fn storage(&self) -> &S;
    fn update_state_hash(&self, state_hash: &H256);
}

/// A main low-level interface to perform state changes
/// for programs.
///
/// Has a main method `update_state` which allows to update program state
/// along with writing the updated state to the storage.
/// By design updates are stored in-memory inside the [`InBlockTransitions`].
pub struct TransitionController<'a, S: Storage> {
    pub storage: &'a S,
    pub transitions: &'a mut InBlockTransitions,
}

impl<S: Storage> TransitionController<'_, S> {
    pub fn update_state<T>(
        &mut self,
        program_id: ActorId,
        f: impl FnOnce(&mut ProgramState, &S, &mut InBlockTransitions) -> T,
    ) -> T {
        let state_hash = self
            .transitions
            .state_of(&program_id)
            .expect("failed to find program in known states")
            .hash;

        let mut state = self
            .storage
            .program_state(state_hash)
            .expect("failed to read state from storage");

        let res = f(&mut state, self.storage, self.transitions);

        let canonical_queue_size = state.canonical_queue.cached_queue_size;
        let injected_queue_size = state.injected_queue.cached_queue_size;
        let new_state_hash = self.storage.write_program_state(state);

        self.transitions.modify_state(
            program_id,
            new_state_hash,
            canonical_queue_size,
            injected_queue_size,
        );

        res
    }
}

pub fn process_queue<S, RI>(
    program_id: ActorId,
    mut program_state: ProgramState,
    queue_type: MessageType,
    instrumented_code: Option<InstrumentedCode>,
    code_metadata: Option<CodeMetadata>,
    ri: &RI,
    gas_allowance: u64,
) -> (ProgramJournals, u64)
where
    S: Storage,
    RI: RuntimeInterface<S>,
    <RI as RuntimeInterface<S>>::LazyPages: Send,
{
    let block_info = ri.block_info();

    log::trace!("Processing {queue_type:?} queue for program {program_id}");

    let is_queue_empty = match queue_type {
        MessageType::Canonical => program_state.canonical_queue.hash.is_empty(),
        MessageType::Injected => program_state.injected_queue.hash.is_empty(),
    };

    if is_queue_empty {
        // Queue is empty, nothing to process.
        return (Vec::new(), 0);
    }

    let queue = program_state
        .queue_from_msg_type(queue_type)
        .hash
        .map(|hash| {
            ri.storage()
                .message_queue(hash)
                .expect("Cannot get message queue")
        })
        .expect("Queue cannot be empty at this point");

    // TODO: must be set by some runtime configuration
    let block_config = BlockConfig {
        block_info,
        forbidden_funcs: [
            // Deprecated
            SyscallName::CreateProgramWGas,
            SyscallName::ReplyCommitWGas,
            SyscallName::ReplyDeposit,
            SyscallName::ReplyInputWGas,
            SyscallName::ReplyWGas,
            SyscallName::ReservationReplyCommit,
            SyscallName::ReservationReply,
            SyscallName::ReservationSendCommit,
            SyscallName::ReservationSend,
            SyscallName::ReserveGas,
            SyscallName::SendCommitWGas,
            SyscallName::SendInputWGas,
            SyscallName::SendWGas,
            SyscallName::SystemReserveGas,
            SyscallName::UnreserveGas,
            // TBD about deprecation
            SyscallName::SignalCode,
            SyscallName::SignalFrom,
            // Temporary forbidden (unimplemented)
            SyscallName::CreateProgram,
            SyscallName::Random,
        ]
        .into(),
        gas_multiplier: GasMultiplier::from_value_per_gas(100),
        costs: Schedule::default().process_costs(),
        max_pages: MAX_WASM_PAGES_AMOUNT.into(),
        outgoing_limit: 1024,
        outgoing_bytes_limit: 64 * 1024 * 1024,
        // TBD about deprecation
        performance_multiplier: Percent::new(100),
        // Deprecated
        existential_deposit: 0,
        mailbox_threshold: 0,
        max_reservations: 0,
        reserve_for: 0,
    };

    let mut mega_journal = Vec::new();
    let mut queue_gas_allowance_counter = GasAllowanceCounter::new(gas_allowance);

    // Create message context settings from block config
    let msg_ctx_settings = ContextSettings {
        sending_fee: block_config.costs.db.write.cost_for(2.into()),
        scheduled_sending_fee: block_config.costs.db.write.cost_for(4.into()),
        waiting_fee: block_config.costs.db.write.cost_for(3.into()),
        waking_fee: block_config.costs.db.write.cost_for(2.into()),
        reservation_fee: block_config.costs.db.write.cost_for(2.into()),
        outgoing_limit: block_config.outgoing_limit,
        outgoing_bytes_limit: block_config.outgoing_bytes_limit,
    };
    let program = match (
        &program_state.program,
        instrumented_code.as_ref(),
        code_metadata.as_ref(),
    ) {
        (state::Program::Active(active_state), Some(code), Some(metadata)) => {
            let allocations = active_state.allocations_hash.map_or_default(|hash| {
                ri.storage()
                    .allocations(hash)
                    .expect("Cannot get allocations")
            });
            Some(core_processor::common::Program {
                id: program_id,
                memory_infix: active_state.memory_infix,
                instrumented_code: code.clone(),
                code_metadata: metadata.clone(),
                allocations: allocations.into(),
            })
        }
        _ => None,
    };

    ri.init_lazy_pages();

    // Cache execution settings once per queue
    let random_data = ri.random_data();
    let execution_settings = core_processor::configs::ExecutionSettings {
        block_info: block_config.block_info,
        performance_multiplier: block_config.performance_multiplier,
        existential_deposit: block_config.existential_deposit,
        mailbox_threshold: block_config.mailbox_threshold,
        max_pages: block_config.max_pages,
        ext_costs: block_config.costs.ext.clone(),
        lazy_pages_costs: block_config.costs.lazy_pages.clone(),
        forbidden_funcs: block_config.forbidden_funcs.clone(),
        reserve_for: block_config.reserve_for,
        random_data,
        gas_multiplier: block_config.gas_multiplier,
    };

    let mut sequence_state = SequenceState::<'_, Ext<RI::LazyPages>>::new();
    let mut memory_snapshot = Ext::<RI::LazyPages>::memory_snapshot();

    for dispatch in queue {
        let origin = dispatch.message_type;
        let call_reply = dispatch.call;
        let is_first_execution = dispatch.context.is_none();

        let journal = process_dispatch(
            dispatch,
            &block_config,
            msg_ctx_settings,
            program_id,
            &program_state,
            &instrumented_code,
            &code_metadata,
            ri,
            queue_gas_allowance_counter.left(),
            &mut sequence_state,
            &mut memory_snapshot,
            program.as_ref(),
            &execution_settings,
        );

        // Check if allocations changed and update the cache
        for note in &journal {
            if let JournalNote::UpdateAllocations {
                program_id: pid,
                allocations,
            } = note
                && *pid == program_id
            {
                sequence_state.update_cached_allocations(allocations.clone());
                break;
            }
        }

        let mut handler = RuntimeJournalHandler {
            storage: ri.storage(),
            program_state: &mut program_state,
            gas_allowance_counter: &mut queue_gas_allowance_counter,
            gas_multiplier: &block_config.gas_multiplier,
            message_type: queue_type,
            is_first_execution,
            stop_processing: false,
        };
        let (unhandled_journal_notes, new_state_hash) = handler.handle_journal(journal);
        mega_journal.push((unhandled_journal_notes, origin, call_reply));

        // Update state hash if it was changed.
        if let Some(new_state_hash) = new_state_hash {
            ri.update_state_hash(&new_state_hash);
        }

        // 'Stop processing' journal note received.
        if handler.stop_processing {
            break;
        }
    }

    let gas_spent = gas_allowance
        .checked_sub(queue_gas_allowance_counter.left())
        .expect("cannot spend more gas than allowed");

    (mega_journal, gas_spent)
}

#[allow(clippy::too_many_arguments)]
fn process_dispatch<'a, S, RI>(
    dispatch: Dispatch,
    block_config: &BlockConfig,
    msg_ctx_settings: ContextSettings,
    program_id: ActorId,
    program_state: &ProgramState,
    instrumented_code: &Option<InstrumentedCode>,
    code_metadata: &Option<CodeMetadata>,
    ri: &RI,
    gas_allowance: u64,
    sequence_state: &mut SequenceState<'a, Ext<RI::LazyPages>>,
    memory_snapshot: &mut impl MemorySnapshot,
    program: Option<&'a Program>,
    execution_settings: &ExecutionSettings,
) -> Vec<JournalNote>
where
    S: Storage,
    RI: RuntimeInterface<S>,
    <RI as RuntimeInterface<S>>::LazyPages: Send,
{
    let Dispatch {
        id: dispatch_id,
        kind,
        source,
        payload,
        value,
        details,
        context,
        ..
    } = dispatch;

    let payload = payload.query(ri.storage()).expect("failed to get payload");

    let gas_limit = block_config
        .gas_multiplier
        .value_to_gas(program_state.executable_balance)
        .min(CHUNK_PROCESSING_GAS_LIMIT);

    let incoming_message =
        IncomingMessage::new(dispatch_id, source, payload, gas_limit, value, details);

    let dispatch = IncomingDispatch::new(kind, incoming_message, context);

    // Use PrechargeContext to determine if we need full charging or can use cached data
    let precharge_ctx = PrechargeContext::new(
        program_id,
        dispatch,
        gas_allowance,
        sequence_state.cached_data(),
    );

    let code = instrumented_code
        .as_ref()
        .expect("Instrumented code must be provided if program is active");
    let code_metadata = code_metadata
        .as_ref()
        .expect("Code metadata must be provided if program is active");

    let (context, allocations_tree) = match precharge_ctx {
        PrechargeContext::NeedsCharging { context } => {
            // Full charging path for first dispatch
            match charge_full_sequence(
                context,
                block_config,
                program_id,
                program_state,
                code,
                code_metadata,
                ri,
                kind,
                dispatch_id,
            ) {
                Ok((ctx, allocs, cache_data)) => {
                    // Cache data for subsequent dispatches
                    sequence_state.cache_execution_data(cache_data);
                    (ctx, allocs)
                }
                Err(journal) => return journal,
            }
        }
        PrechargeContext::PreCharged { context } => {
            // Pre-charged path - validate program state only
            if let Err(journal) =
                validate_program_state_precharged(program_state, kind, dispatch_id, program_id)
            {
                return journal;
            }

            // Use cached allocations
            let cached = sequence_state
                .cached_data()
                .expect("cached data must exist for PreCharged context");
            (context, cached.actor_data.allocations.clone())
        }
    };

    // Call into_final_parts() directly and use the already-existing program reference
    let (
        _destination_id,
        dispatch,
        gas_counter,
        gas_allowance_counter,
        actor_data,
        allocations_data,
    ) = context.into_final_parts();

    let program = program.expect("program must be initialized for execution");

    // Create gas reserver from dispatch and actor data
    let gas_reserver = gear_core::reservation::GasReserver::new(
        &dispatch,
        actor_data.gas_reservation_map,
        allocations_data.max_reservations,
    );

    let balance = program_state.balance;
    let memory_size = allocations_data.memory_size;

    let initial_reservations_amount = gas_reserver.states().len();
    let dispatch_for_journal = dispatch.clone();
    let system_reservation_ctx =
        core_processor::SystemReservationContext::from_dispatch(&dispatch_for_journal);

    let execution_step = ExecutionStep {
        balance,
        dispatch,
        allocations: allocations_tree,
        gas_counter,
        gas_allowance_counter,
        gas_reserver,
    };

    let exec_result = execute_wasm_step::<Ext<RI::LazyPages>, _>(
        execution_step,
        program,
        memory_size,
        execution_settings,
        msg_ctx_settings,
        sequence_state,
        memory_snapshot,
    );

    core_processor::process_execution_result(
        dispatch_for_journal,
        program_id,
        initial_reservations_amount,
        system_reservation_ctx,
        exec_result,
    )
    .unwrap_or_else(|err| unreachable!("{err}"))
}

/// Performs full charging sequence for first dispatch.
/// Returns the charged context, allocations tree, and data to cache for subsequent dispatches.
#[allow(clippy::too_many_arguments)]
fn charge_full_sequence<S, RI>(
    context: ContextCharged<core_processor::ForNothing>,
    block_config: &BlockConfig,
    program_id: ActorId,
    program_state: &ProgramState,
    code: &InstrumentedCode,
    code_metadata: &CodeMetadata,
    ri: &RI,
    kind: DispatchKind,
    dispatch_id: gprimitives::MessageId,
) -> Result<
    (
        ContextCharged<core_processor::ForModuleInstantiation>,
        IntervalsTree<WasmPage>,
        CachedExecutionData,
    ),
    Vec<JournalNote>,
>
where
    S: Storage,
    RI: RuntimeInterface<S>,
{
    let context = context.charge_for_program(block_config)?;

    let active_state = match &program_state.program {
        state::Program::Active(state) => state,
        state::Program::Terminated(pid) => {
            log::trace!("Program {pid} has failed init");
            return Err(core_processor::process_failed_init(context));
        }
        state::Program::Exited(pid) => {
            log::trace!("Program {pid} has exited");
            return Err(core_processor::process_program_exited(context, *pid));
        }
    };

    if active_state.initialized && kind == DispatchKind::Init {
        unreachable!(
            "Init message {dispatch_id} is sent to already initialized program {program_id}",
        );
    }

    if !active_state.initialized && !matches!(kind, DispatchKind::Init | DispatchKind::Reply) {
        log::trace!(
            "Program {program_id} is not yet finished initialization, so cannot process handle message"
        );
        return Err(core_processor::process_uninitialized(context));
    }

    let context = context.charge_for_code_metadata(block_config)?;

    let context = context.charge_for_instrumented_code(block_config, code.bytes().len() as u32)?;

    let allocations = active_state.allocations_hash.map_or_default(|hash| {
        ri.storage()
            .allocations(hash)
            .expect("Cannot get allocations")
    });

    let context = context.charge_for_allocations(block_config, allocations.tree_len())?;

    let allocations_tree: IntervalsTree<WasmPage> = allocations.into();

    let actor_data = ExecutableActorData {
        allocations: allocations_tree.clone(),
        gas_reservation_map: Default::default(),
        memory_infix: active_state.memory_infix,
    };

    // Compute memory size for caching
    let memory_size = allocations_tree
        .end()
        .map(|p| p.inc())
        .unwrap_or_else(|| code_metadata.static_pages());

    let cache_data = CachedExecutionData {
        actor_data: actor_data.clone(),
        reservations_and_memory_size: ReservationsAndMemorySize {
            max_reservations: block_config.max_reservations,
            memory_size,
        },
    };

    let context = context.charge_for_module_instantiation(
        block_config,
        actor_data,
        code.instantiated_section_sizes(),
        code_metadata,
    )?;

    Ok((context, allocations_tree, cache_data))
}

/// Validates program state for pre-charged path (subsequent dispatches).
/// Returns Err with empty journal if program is invalid (caller should handle terminated/exited).
fn validate_program_state_precharged(
    program_state: &ProgramState,
    kind: DispatchKind,
    dispatch_id: gprimitives::MessageId,
    program_id: ActorId,
) -> Result<(), Vec<JournalNote>> {
    let active_state = match &program_state.program {
        state::Program::Active(state) => state,
        state::Program::Terminated(_) | state::Program::Exited(_) => {
            // Program state changed between dispatches - this shouldn't happen
            // in a well-formed queue, but we handle it gracefully.
            // Return empty journal to skip this dispatch.
            log::warn!(
                "Program {program_id} state changed to terminated/exited during sequence processing"
            );
            return Err(Vec::new());
        }
    };

    if active_state.initialized && kind == DispatchKind::Init {
        unreachable!(
            "Init message {dispatch_id} is sent to already initialized program {program_id}",
        );
    }

    if !active_state.initialized && !matches!(kind, DispatchKind::Init | DispatchKind::Reply) {
        log::trace!(
            "Program {program_id} is not yet finished initialization, so cannot process handle message"
        );
        // For precharged context, we can't call process_uninitialized because we don't have
        // a ForProgram context. Return empty journal - the program state validation
        // should have been done on first dispatch.
        return Err(Vec::new());
    }

    Ok(())
}

pub const fn pack_u32_to_i64(low: u32, high: u32) -> i64 {
    let mut result = 0u64;
    result |= (high as u64) << 32;
    result |= low as u64;
    result as i64
}

pub const fn unpack_i64_to_u32(val: i64) -> (u32, u32) {
    let val = val as u64;
    let high = (val >> 32) as u32;
    let low = val as u32;
    (low, high)
}
