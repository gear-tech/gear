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
    common::{ExecutableActorData, JournalNote},
    configs::{BlockConfig, SyscallName},
    ContextChargedForCode, ContextChargedForInstrumentation, Ext, ProcessExecutionContext,
};
use ethexe_common::gear::{Origin, CHUNK_PROCESSING_GAS_LIMIT};
use gear_core::{
    code::{InstrumentedCode, MAX_WASM_PAGES_AMOUNT},
    gas::GasAllowanceCounter,
    ids::ActorId,
    message::{DispatchKind, IncomingDispatch, IncomingMessage},
};
use gear_lazy_pages_common::LazyPagesInterface;
use gprimitives::{CodeId, H256};
use gsys::{GasMultiplier, Percent};
use journal::RuntimeJournalHandler;
use state::{Dispatch, ProgramState, Storage};

pub use core_processor::configs::BlockInfo;
pub use journal::NativeJournalHandler as JournalHandler;
pub use schedule::{Handler as ScheduleHandler, Restorer as ScheduleRestorer};
pub use transitions::{InBlockTransitions, NonFinalTransition};

pub mod state;

mod journal;
mod schedule;
mod transitions;

pub const RUNTIME_ID: u32 = 0;

pub type ProgramJournals = Vec<(Vec<JournalNote>, Origin, bool)>;

pub trait RuntimeInterface<S: Storage> {
    type LazyPages: LazyPagesInterface + 'static;

    fn block_info(&self) -> BlockInfo;
    fn init_lazy_pages(&self);
    fn random_data(&self) -> (Vec<u8>, u32);
    fn storage(&self) -> &S;
    fn update_state_hash(&self, state_hash: &H256);
}

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
            .read_state(state_hash)
            .expect("failed to read state from storage");

        let res = f(&mut state, self.storage, self.transitions);

        let queue_size = state.queue.cached_queue_size;
        let new_state_hash = self.storage.write_state(state);

        self.transitions
            .modify_state(program_id, new_state_hash, queue_size);

        res
    }
}

pub fn process_queue<S, RI>(
    program_id: ActorId,
    mut program_state: ProgramState,
    instrumented_code: Option<InstrumentedCode>,
    code_id: CodeId,
    ri: &RI,
    gas_allowance: u64,
) -> (ProgramJournals, u64)
where
    S: Storage,
    RI: RuntimeInterface<S>,
    <RI as RuntimeInterface<S>>::LazyPages: Send,
{
    let block_info = ri.block_info();

    log::trace!("Processing queue for program {program_id}");

    if program_state.queue.hash.is_empty() {
        // Queue is empty, nothing to process.
        return (Vec::new(), 0);
    }

    let queue = program_state
        .queue
        .hash
        .map(|hash| {
            ri.storage()
                .read_queue(hash)
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
        costs: Default::default(),
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

    ri.init_lazy_pages();

    for dispatch in queue {
        let origin = dispatch.origin;
        let call_reply = dispatch.call;

        let journal = process_dispatch(
            dispatch,
            &block_config,
            program_id,
            &program_state,
            &instrumented_code,
            code_id,
            ri,
            queue_gas_allowance_counter.left(),
        );
        let mut handler = RuntimeJournalHandler {
            storage: ri.storage(),
            program_state: &mut program_state,
            gas_allowance_counter: &mut queue_gas_allowance_counter,
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
fn process_dispatch<S, RI>(
    dispatch: Dispatch,
    block_config: &BlockConfig,
    program_id: ActorId,
    program_state: &ProgramState,
    instrumented_code: &Option<InstrumentedCode>,
    code_id: CodeId,
    ri: &RI,
    gas_allowance: u64,
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

    let context = match core_processor::precharge_for_program(
        block_config,
        gas_allowance,
        dispatch,
        program_id,
    ) {
        Ok(dispatch) => dispatch,
        Err(journal) => return journal,
    };

    let active_state = match &program_state.program {
        state::Program::Active(state) => state,
        state::Program::Terminated(program_id) => {
            log::trace!("Program {program_id} has failed init");
            return core_processor::process_failed_init(context);
        }
        state::Program::Exited(program_id) => {
            log::trace!("Program {program_id} has exited");
            return core_processor::process_program_exited(context, *program_id);
        }
    };

    if active_state.initialized && kind == DispatchKind::Init {
        // Panic is impossible, because gear protocol does not provide functionality
        // to send second init message to any already existing program.
        unreachable!(
            "Init message {dispatch_id} is sent to already initialized program {program_id}",
        );
    }

    // If the destination program is uninitialized, then we allow
    // to process message, if it's a reply or init message.
    // Otherwise, we return error reply.
    if !active_state.initialized && !matches!(kind, DispatchKind::Init | DispatchKind::Reply) {
        log::trace!("Program {program_id} is not yet finished initialization, so cannot process handle message");
        return core_processor::process_uninitialized(context);
    }

    // TODO: support normal allocations len #4068
    let allocations = active_state.allocations_hash.map_or_default(|hash| {
        ri.storage()
            .read_allocations(hash)
            .expect("Cannot get allocations")
    });

    let context = match core_processor::precharge_for_allocations(
        block_config,
        context,
        allocations.tree_len(),
    ) {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let code = instrumented_code
        .as_ref()
        .expect("Instrumented code must be provided if program is active");

    let actor_data = ExecutableActorData {
        allocations: allocations.into(),
        code_id,
        code_exports: code.exports().clone(),
        static_pages: code.static_pages(),
        gas_reservation_map: Default::default(), // TODO (gear_v2): deprecate it.
        memory_infix: active_state.memory_infix,
    };

    let context = match core_processor::precharge_for_code_length(block_config, context, actor_data)
    {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let context = ContextChargedForCode::from(context);
    let context = ContextChargedForInstrumentation::from(context);
    let context = match core_processor::precharge_for_module_instantiation(
        block_config,
        context,
        code.instantiated_section_sizes(),
    ) {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let execution_context =
        ProcessExecutionContext::from((context, code.clone(), program_state.balance));

    let random_data = ri.random_data();

    core_processor::process::<Ext<RI::LazyPages>>(block_config, execution_context, random_data)
        .unwrap_or_else(|err| unreachable!("{err}"))
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
