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
};
use gear_lazy_pages_common::LazyPagesInterface;
use gprimitives::H256;
use gsys::{GasMultiplier, Percent};
use journal::RuntimeJournalHandler;
use state::{Dispatch, ProgramState, Storage};

pub use core_processor::configs::BlockInfo;
use gear_core::code::InstrumentedCodeAndMetadata;
pub use journal::NativeJournalHandler as JournalHandler;
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

/// Configuration for Ethereum-related constants.
pub struct EthereumConfig {
    /// The amount of gas charged for the first message in announce.
    first_message_fee: u64,
}

/// Processes the program message queue of given type.
///
/// Panics if the queue is empty. It's needed to guarantee
/// that the function always charges for the first message.
///
/// Returns journals and the amount of gas burned.
//
// TODO: refactor the function to reduce the number of arguments (#5100)
#[allow(clippy::too_many_arguments)]
pub fn process_queue<S, RI>(
    program_id: ActorId,
    mut program_state: ProgramState,
    queue_type: MessageType,
    instrumented_code: Option<InstrumentedCode>,
    code_metadata: Option<CodeMetadata>,
    ri: &RI,
    is_first_queue: bool,
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

    assert!(
        !is_queue_empty,
        "the function must not be run with empty queues"
    );

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

    // TODO: must be set somewhere by some runtime configuration
    let ethereum_config = EthereumConfig {
        // TODO: the value of the fee must be something sensible,
        //       not just some arbitrary number.
        first_message_fee: 1000,
    };

    let mut mega_journal = Vec::new();
    let mut queue_gas_allowance_counter = GasAllowanceCounter::new(gas_allowance);

    ri.init_lazy_pages();

    for (i, dispatch) in queue.into_iter().enumerate() {
        let origin = dispatch.message_type;
        let call_reply = dispatch.call;
        let is_first_message = i == 0 && is_first_queue;
        let is_first_execution = dispatch.context.is_none();

        let (Ok(journal) | Err(journal)) = process_dispatch(
            dispatch,
            &block_config,
            &ethereum_config,
            program_id,
            &program_state,
            is_first_message,
            &instrumented_code,
            &code_metadata,
            ri,
            queue_gas_allowance_counter.left(),
        );
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

// TODO: refactor the function to reduce the number of arguments (#5100)
#[allow(clippy::too_many_arguments)]
fn process_dispatch<S, RI>(
    dispatch: Dispatch,
    block_config: &BlockConfig,
    ethereum_config: &EthereumConfig,
    program_id: ActorId,
    program_state: &ProgramState,
    is_first_message: bool,
    instrumented_code: &Option<InstrumentedCode>,
    code_metadata: &Option<CodeMetadata>,
    ri: &RI,
    gas_allowance: u64,
) -> Result<Vec<JournalNote>, Vec<JournalNote>>
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

    let mut context = ContextCharged::new(program_id, dispatch, gas_allowance);

    if is_first_message {
        context = context.charge_extra_fee(
            "process the first message",
            false,
            ethereum_config.first_message_fee,
        )?;
    }

    let context = context.charge_for_program(block_config)?;

    let active_state = match &program_state.program {
        state::Program::Active(state) => state,
        state::Program::Terminated(program_id) => {
            log::trace!("Program {program_id} has failed init");
            return Err(core_processor::process_failed_init(context));
        }
        state::Program::Exited(program_id) => {
            log::trace!("Program {program_id} has exited");
            return Err(core_processor::process_program_exited(context, *program_id));
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
        return Err(core_processor::process_uninitialized(context));
    }

    let context = context.charge_for_code_metadata(block_config)?;

    let code = instrumented_code
        .as_ref()
        .expect("Instrumented code must be provided if program is active");
    let code_metadata = code_metadata
        .as_ref()
        .expect("Code metadata must be provided if program is active");

    let context = context.charge_for_instrumented_code(block_config, code.bytes().len() as u32)?;

    let allocations = active_state.allocations_hash.map_or_default(|hash| {
        ri.storage()
            .allocations(hash)
            .expect("Cannot get allocations")
    });

    let context = context.charge_for_allocations(block_config, allocations.tree_len())?;

    let actor_data = ExecutableActorData {
        allocations: allocations.into(),
        gas_reservation_map: Default::default(), // TODO (gear_v2): deprecate it.
        memory_infix: active_state.memory_infix,
    };

    let context = context.charge_for_module_instantiation(
        block_config,
        actor_data,
        code.instantiated_section_sizes(),
        code_metadata,
    )?;

    let execution_context = ProcessExecutionContext::new(
        context,
        InstrumentedCodeAndMetadata {
            instrumented_code: code.clone(),
            metadata: code_metadata.clone(),
        },
        program_state.balance,
    );

    let random_data = ri.random_data();

    Ok(
        core_processor::process::<Ext<RI::LazyPages>>(block_config, execution_context, random_data)
            .unwrap_or_else(|err| unreachable!("{err}")),
    )
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
