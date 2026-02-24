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
    ContextCharged, Ext, ProcessExecutionContext,
    common::{ExecutableActorData, JournalNote},
    configs::{BlockConfig, SyscallName},
};
use ethexe_common::gear::{CHUNK_PROCESSING_GAS_LIMIT, MessageType};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode, MAX_WASM_PAGES_AMOUNT},
    gas::GasAllowanceCounter,
    gas_metering::Schedule,
    ids::ActorId,
    message::{DispatchKind, IncomingDispatch, IncomingMessage},
    rpc::ReplyInfo,
};
use gear_lazy_pages_common::LazyPagesInterface;
use gprimitives::{H256, MessageId};
use gsys::{GasMultiplier, Percent};
use journal::RuntimeJournalHandler;
use state::{Dispatch, ProgramState, Storage};

pub use core_processor::configs::BlockInfo;
use gear_core::code::InstrumentedCodeAndMetadata;
pub use journal::NativeJournalHandler as JournalHandler;
use parity_scale_codec::{Decode, Encode};
pub use schedule::{Handler as ScheduleHandler, Restorer as ScheduleRestorer};
pub use transitions::{FinalizedBlockTransitions, InBlockTransitions, NonFinalTransition};

pub mod state;

mod journal;
mod schedule;
mod transitions;

// TODO: consider format.
/// Version of the runtime.
pub const VERSION: u32 = 1;
pub const RUNTIME_ID: u32 = 1;

pub type ProgramJournals = Vec<(Vec<JournalNote>, MessageType, bool)>;

/// Context passed to the runtime in order to
/// run message queue processing for specified program.
#[derive(Debug, Encode, Decode)]
pub struct ProcessQueueContext {
    pub program_id: ActorId,
    pub state_root: H256,
    pub queue_type: MessageType,
    pub instrumented_code: InstrumentedCode,
    pub code_metadata: CodeMetadata,
    pub gas_allowance: GasAllowanceCounter,
    pub block_info: BlockInfo,
    // pub promise_sender: mpsc::Sender<Promise>,
}

pub trait RuntimeInterface: Storage {
    type LazyPages: LazyPagesInterface + 'static;

    fn init_lazy_pages(&self);
    fn random_data(&self) -> (Vec<u8>, u32);
    fn update_state_hash(&self, state_hash: &H256);
    // TODO: create more meaningful function name
    fn send_promise(&self, reply: &ReplyInfo, message_id: &MessageId);
}

/// A main low-level interface to perform state changes
/// for programs.
///
/// Has a main method `update_state` which allows to update program state
/// along with writing the updated state to the storage.
/// By design updates are stored in-memory inside the [`InBlockTransitions`].
pub struct TransitionController<'a, S: Storage + ?Sized> {
    pub storage: &'a S,
    pub transitions: &'a mut InBlockTransitions,
}

impl<S: Storage + ?Sized> TransitionController<'_, S> {
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

pub fn process_queue<RI>(mut ctx: ProcessQueueContext, ri: &RI) -> (ProgramJournals, u64)
where
    RI: RuntimeInterface,
    RI::LazyPages: Send,
{
    let mut program_state = ri.program_state(ctx.state_root).unwrap();

    let ProcessQueueContext {
        program_id,
        queue_type,
        ..
    } = ctx;

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
        .map(|hash| ri.message_queue(hash).expect("Cannot get message queue"))
        .expect("Queue cannot be empty at this point");

    // TODO: must be set by some runtime configuration
    let block_config = BlockConfig {
        block_info: ctx.block_info,
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
    let initial_gas_allowance = ctx.gas_allowance.left();

    ri.init_lazy_pages();

    for dispatch in queue {
        let dispatch_id = dispatch.id;
        let message_type = dispatch.message_type;
        let call_reply = dispatch.call;
        let is_first_execution = dispatch.context.is_none();
        let is_promise_required = dispatch.kind.is_handle() && dispatch.message_type.is_injected();

        let journal = process_dispatch(dispatch, &block_config, &program_state, &ctx, ri);
        let mut handler = RuntimeJournalHandler {
            storage: ri,
            program_state: &mut program_state,
            gas_allowance_counter: &mut ctx.gas_allowance,
            gas_multiplier: &block_config.gas_multiplier,
            message_type: ctx.queue_type,
            is_first_execution,
            stop_processing: false,
        };

        log::error!("processing dispatch: message_type={message_type:?}");
        if is_promise_required {
            log::error!("promise required, notes={journal:?}");
            for note in journal.iter() {
                if let JournalNote::SendDispatch {
                    message_id,
                    dispatch,
                    ..
                } = note
                    && *message_id == dispatch_id
                    && dispatch.kind().is_reply()
                {
                    let code = dispatch
                        .reply_details()
                        .map(|d| d.to_reply_code())
                        .expect("reply details must exists for reply dispatch");

                    let reply = ReplyInfo {
                        value: dispatch.value(),
                        code,
                        payload: dispatch.message().payload_bytes().to_vec(),
                    };

                    log::error!("sending promise to user");
                    ri.send_promise(&reply, &dispatch_id);
                    break;
                }
            }
        }
        let (unhandled_journal_notes, new_state_hash) = handler.handle_journal(journal);
        mega_journal.push((unhandled_journal_notes, message_type, call_reply));

        // Update state hash if it was changed.
        if let Some(new_state_hash) = new_state_hash {
            ri.update_state_hash(&new_state_hash);
        }

        // 'Stop processing' journal note received.
        if handler.stop_processing {
            break;
        }
    }

    let gas_spent = initial_gas_allowance
        .checked_sub(ctx.gas_allowance.left())
        .expect("cannot spend more gas than allowed");

    (mega_journal, gas_spent)
}

fn process_dispatch<RI>(
    dispatch: Dispatch,
    block_config: &BlockConfig,
    program_state: &ProgramState,
    ctx: &ProcessQueueContext,
    ri: &RI,
) -> Vec<JournalNote>
where
    RI: RuntimeInterface,
    RI::LazyPages: Send,
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

    let &ProcessQueueContext {
        program_id,
        instrumented_code: ref code,
        ref code_metadata,
        ..
    } = ctx;

    let payload = payload.query(ri).expect("failed to get payload");

    let gas_limit = block_config
        .gas_multiplier
        .value_to_gas(program_state.executable_balance)
        .min(CHUNK_PROCESSING_GAS_LIMIT);

    let incoming_message =
        IncomingMessage::new(dispatch_id, source, payload, gas_limit, value, details);

    let dispatch = IncomingDispatch::new(kind, incoming_message, context);

    let context = ContextCharged::new(program_id, dispatch, ctx.gas_allowance.left());

    let context = match context.charge_for_program(block_config) {
        Ok(context) => context,
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
        log::trace!(
            "Program {program_id} is not yet finished initialization, so cannot process handle message"
        );
        return core_processor::process_uninitialized(context);
    }

    let context = match context.charge_for_code_metadata(block_config) {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let context =
        match context.charge_for_instrumented_code(block_config, code.bytes().len() as u32) {
            Ok(context) => context,
            Err(journal) => return journal,
        };

    let allocations = active_state
        .allocations_hash
        .map_or_default(|hash| ri.allocations(hash).expect("Cannot get allocations"));

    let context = match context.charge_for_allocations(block_config, allocations.tree_len()) {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let actor_data = ExecutableActorData {
        allocations: allocations.into(),
        gas_reservation_map: Default::default(), // TODO (gear_v2): deprecate it.
        memory_infix: active_state.memory_infix,
    };

    let context = match context.charge_for_module_instantiation(
        block_config,
        actor_data,
        code.instantiated_section_sizes(),
        code_metadata,
    ) {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let execution_context = ProcessExecutionContext::new(
        context,
        InstrumentedCodeAndMetadata {
            instrumented_code: code.clone(),
            metadata: code_metadata.clone(),
        },
        program_state.balance,
    );

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
