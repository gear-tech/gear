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
#![allow(unused)]

use alloc::{
    collections::{BTreeMap, VecDeque},
    vec::Vec,
};
use core::{marker::PhantomData, mem::swap};
use core_processor::{
    common::{ExecutableActorData, JournalNote},
    configs::{BlockConfig, SyscallName},
    ContextChargedForCode, ContextChargedForInstrumentation, Ext, ProcessExecutionContext,
};
use gear_core::{
    code::InstrumentedCode,
    ids::ProgramId,
    message::{DispatchKind, IncomingDispatch, IncomingMessage, Value},
    pages::{numerated::tree::IntervalsTree, GearPage, WasmPage},
    program::MemoryInfix,
    reservation::GasReservationMap,
};
use gear_lazy_pages_common::LazyPagesInterface;
use gprimitives::{CodeId, H256};
use gsys::{GasMultiplier, Percent};
use parity_scale_codec::{Decode, Encode};
use state::{
    ActiveProgram, Dispatch, HashAndLen, InitStatus, MaybeHash, MessageQueue, ProgramState,
    Storage, Waitlist,
};

extern crate alloc;

mod journal;
pub mod state;

pub use core_processor::configs::BlockInfo;
pub use journal::Handler;

const RUNTIME_ID: u32 = 0;

pub const BLOCK_GAS_LIMIT: u64 = 1_000_000_000_000;

pub trait RuntimeInterface<S: Storage> {
    type LazyPages: LazyPagesInterface + 'static;

    fn block_info(&self) -> BlockInfo;
    fn init_lazy_pages(&self, pages_map: BTreeMap<GearPage, H256>);
    fn random_data(&self) -> (Vec<u8>, u32);
    fn storage(&self) -> &S;
}

pub fn wake_messages<S: Storage, RI: RuntimeInterface<S>>(
    _program_id: ProgramId,
    _program_state: ProgramState,
    _ri: &RI,
) -> Option<H256> {
    todo!("breathx");
    // let block_info = ri.block_info();

    // let mut queue = program_state.queue_hash.with_hash_or_default(|hash| {
    //     ri.storage()
    //         .read_queue(hash)
    //         .expect("Cannot get message queue")
    // });

    // let mut waitlist = match program_state.waitlist_hash {
    //     MaybeHash::Empty => {
    //         // No messages in waitlist
    //         return None;
    //     }
    //     MaybeHash::Hash(HashAndLen { hash, .. }) => ri
    //         .storage()
    //         .read_waitlist(hash)
    //         .expect("Cannot get waitlist"),
    // };

    // let mut dispatches_to_wake = Vec::new();
    // let mut remove_from_waitlist_blocks = Vec::new();
    // for (block, list) in waitlist.range_mut(0..=block_info.height) {
    //     if list.is_empty() {
    //         log::error!("Empty waitlist for block, must been removed from waitlist")
    //     }
    //     dispatches_to_wake.append(list);
    //     remove_from_waitlist_blocks.push(*block);
    // }

    // if remove_from_waitlist_blocks.is_empty() {
    //     // No messages to wake up
    //     return None;
    // }

    // for block in remove_from_waitlist_blocks {
    //     waitlist.remove(&block);
    // }

    // for dispatch in dispatches_to_wake {
    //     queue.push_back(dispatch);
    // }

    // let queue_hash = ri.storage().write_queue(queue).into();
    // let waitlist_hash = ri.storage().write_waitlist(waitlist).into();

    // let new_program_state = ProgramState {
    //     queue_hash,
    //     waitlist_hash,
    //     ..program_state
    // };

    // Some(ri.storage().write_state(new_program_state))
}

pub fn process_next_message<S: Storage, RI: RuntimeInterface<S>>(
    program_id: ProgramId,
    program_state: ProgramState,
    instrumented_code: Option<InstrumentedCode>,
    code_id: CodeId,
    ri: &RI,
) -> Vec<JournalNote> {
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
        performance_multiplier: Percent::new(100),
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
            // TODO: refactor asap
            SyscallName::GasAvailable,
            // Temporary forbidden (unimplemented)
            SyscallName::CreateProgram,
            SyscallName::Exit,
            SyscallName::Random,
        ]
        .into(),
        reserve_for: 125_000_000,
        gas_multiplier: GasMultiplier::one(), // TODO
        costs: Default::default(),            // TODO
        existential_deposit: 0,               // TODO
        mailbox_threshold: 3000,
        max_reservations: 50,
        max_pages: 512.into(),
        outgoing_limit: 1024,
        outgoing_bytes_limit: 64 * 1024 * 1024,
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
        payload_hash,
        value,
        details,
        context,
    } = queue.pop_front().unwrap();

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

    let payload = payload_hash
        .with_hash_or_default(|hash| ri.storage().read_payload(hash).expect("Cannot get payload"));

    let gas_limit = block_config
        .gas_multiplier
        .value_to_gas(program_state.executable_balance)
        .min(BLOCK_GAS_LIMIT);

    let incoming_message =
        IncomingMessage::new(dispatch_id, source, payload, gas_limit, value, details);

    let dispatch = IncomingDispatch::new(kind, incoming_message, context);

    let context = match core_processor::precharge_for_program(
        &block_config,
        1_000_000_000_000,
        dispatch,
        program_id,
    ) {
        Ok(dispatch) => dispatch,
        Err(journal) => return journal,
    };

    let code = instrumented_code.expect("Instrumented code must be provided if program is active");

    // TODO: support normal allocations len #4068
    let allocations = active_state.allocations_hash.with_hash_or_default(|hash| {
        ri.storage()
            .read_allocations(hash)
            .expect("Cannot get allocations")
    });

    let context = match core_processor::precharge_for_allocations(
        &block_config,
        context,
        allocations.intervals_amount() as u32,
    ) {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let pages_map = active_state.pages_hash.with_hash_or_default(|hash| {
        ri.storage()
            .read_pages(hash)
            .expect("Cannot get memory pages")
    });
    let actor_data = ExecutableActorData {
        allocations,
        code_id,
        code_exports: code.exports().clone(),
        static_pages: code.static_pages(),
        gas_reservation_map: Default::default(), // TODO (gear_v2): deprecate it.
        memory_infix: active_state.memory_infix,
    };

    let context =
        match core_processor::precharge_for_code_length(&block_config, context, actor_data) {
            Ok(context) => context,
            Err(journal) => return journal,
        };

    let context = ContextChargedForCode::from(context);
    let context = ContextChargedForInstrumentation::from(context);
    let context = match core_processor::precharge_for_module_instantiation(
        &block_config,
        context,
        code.instantiated_section_sizes(),
    ) {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let execution_context = ProcessExecutionContext::from((context, code, program_state.balance));

    let random_data = ri.random_data();

    ri.init_lazy_pages(pages_map.clone());

    core_processor::process::<Ext<RI::LazyPages>>(&block_config, execution_context, random_data)
        .unwrap_or_else(|err| unreachable!("{err}"))
}
