// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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
    ContextCharged, Ext, ProcessExecutionContext,
};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode, MAX_WASM_PAGES_AMOUNT},
    ids::ProgramId,
    message::{DispatchKind, IncomingDispatch, IncomingMessage},
};
use gear_lazy_pages_common::LazyPagesInterface;
use gsys::{GasMultiplier, Percent};
use state::{Dispatch, ProgramState, Storage};

pub use core_processor::configs::BlockInfo;
use gear_core::code::InstrumentedCodeAndMetadata;
pub use journal::Handler as JournalHandler;
pub use schedule::Handler as ScheduleHandler;
pub use transitions::{InBlockTransitions, NonFinalTransition};

pub mod state;

mod journal;
mod schedule;
mod transitions;

pub const BLOCK_GAS_LIMIT: u64 = 1_000_000_000_000;

pub const RUNTIME_ID: u32 = 0;

pub trait RuntimeInterface<S: Storage> {
    type LazyPages: LazyPagesInterface + 'static;

    fn block_info(&self) -> BlockInfo;
    fn init_lazy_pages(&self);
    fn random_data(&self) -> (Vec<u8>, u32);
    fn storage(&self) -> &S;
}

pub struct TransitionController<'a, S: Storage> {
    pub storage: &'a S,
    pub transitions: &'a mut InBlockTransitions,
}

impl<'a, S: Storage> TransitionController<'a, S> {
    pub fn update_state<T>(
        &mut self,
        program_id: ProgramId,
        f: impl FnOnce(&mut ProgramState, &S, &mut InBlockTransitions) -> T,
    ) -> T {
        let state_hash = self
            .transitions
            .state_of(&program_id)
            .expect("failed to find program in known states");

        let mut state = self
            .storage
            .read_state(state_hash)
            .expect("failed to read state from storage");

        let res = f(&mut state, self.storage, self.transitions);

        let new_state_hash = self.storage.write_state(state);

        self.transitions.modify_state(program_id, new_state_hash);

        res
    }
}

pub fn process_next_message<S, RI>(
    program_id: ProgramId,
    program_state: ProgramState,
    instrumented_code: Option<InstrumentedCode>,
    code_metadata: Option<CodeMetadata>,
    ri: &RI,
) -> Vec<JournalNote>
where
    S: Storage,
    RI: RuntimeInterface<S>,
    <RI as RuntimeInterface<S>>::LazyPages: Send,
{
    let block_info = ri.block_info();

    log::trace!("Processing next message for program {program_id}");

    let mut queue = program_state.queue_hash.with_hash_or_default(|hash| {
        ri.storage()
            .read_queue(hash)
            .expect("Cannot get message queue")
    });

    if queue.is_empty() {
        return Vec::new();
    }

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
        gas_multiplier: GasMultiplier::one(),
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

    let active_state = match program_state.program {
        state::Program::Active(state) => state,
        state::Program::Exited(program_id) | state::Program::Terminated(program_id) => {
            log::trace!("Program {program_id} is not active");
            todo!("Support non-active program")
        }
    };

    let Dispatch {
        id: dispatch_id,
        kind,
        source,
        payload,
        value,
        details,
        context,
    } = queue.dequeue().unwrap(); // TODO (breathx): why unwrap?

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
        todo!("Process messages to uninitialized program");
    }

    let payload = payload.query(ri.storage()).expect("failed to get payload");

    let gas_limit = block_config
        .gas_multiplier
        .value_to_gas(program_state.executable_balance)
        .min(BLOCK_GAS_LIMIT);

    let incoming_message =
        IncomingMessage::new(dispatch_id, source, payload, gas_limit, value, details);

    let dispatch = IncomingDispatch::new(kind, incoming_message, context);

    let context = ContextCharged::new(program_id, dispatch, 1_000_000_000_000);

    let context = match context.charge_for_program(&block_config) {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let context = match context.charge_for_code_metadata(&block_config) {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let code = instrumented_code.expect("Instrumented code must be provided if program is active");
    let code_metadata = code_metadata.expect("Code metadata must be provided if program is active");

    let context =
        match context.charge_for_instrumented_code(&block_config, code.bytes().len() as u32) {
            Ok(context) => context,
            Err(journal) => return journal,
        };

    // TODO: support normal allocations len #4068
    let allocations = active_state.allocations_hash.with_hash_or_default(|hash| {
        ri.storage()
            .read_allocations(hash)
            .expect("Cannot get allocations")
    });

    let context = match context.charge_for_allocations(&block_config, allocations.tree_len()) {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let actor_data = ExecutableActorData {
        allocations: allocations.into(),
        gas_reservation_map: Default::default(), // TODO (gear_v2): deprecate it.
        memory_infix: active_state.memory_infix,
    };

    let context = match context.charge_for_module_instantiation(
        &block_config,
        actor_data,
        code.instantiated_section_sizes(),
        &code_metadata,
    ) {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let execution_context = ProcessExecutionContext::new(
        context,
        InstrumentedCodeAndMetadata {
            instrumented_code: code,
            metadata: code_metadata,
        },
        program_state.balance,
    );

    let random_data = ri.random_data();

    ri.init_lazy_pages();

    core_processor::process::<Ext<RI::LazyPages>>(&block_config, execution_context, random_data)
        .unwrap_or_else(|err| unreachable!("{err}"))
}
