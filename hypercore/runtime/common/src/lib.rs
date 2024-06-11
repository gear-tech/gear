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
    configs::{BlockConfig, BlockInfo},
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
use receipts::Receipt;
use state::{
    ActiveProgram, Dispatch, HashAndLen, InitStatus, MaybeHash, MessageQueue, ProgramState,
    Storage, Waitlist,
};

extern crate alloc;

mod journal;
pub mod receipts;
pub mod state;

const RUNTIME_ID: u32 = 0;

pub trait RuntimeInterface<S: Storage> {
    type LazyPages: LazyPagesInterface + 'static;

    fn block_info(&self) -> BlockInfo;
    fn init_lazy_pages(&self, pages_map: BTreeMap<GearPage, H256>);
    fn random_data(&self) -> (Vec<u8>, u32);
    fn storage(&self) -> &S;
}

struct ExecutableProgramContext {
    allocations: IntervalsTree<WasmPage>,
    code: InstrumentedCode,
    gas_reservation_map: GasReservationMap,
    memory_infix: MemoryInfix,
    pages_map: BTreeMap<GearPage, H256>,
    status: InitStatus,
    balance: Value,
}

enum ProgramContext {
    Executable(ExecutableProgramContext),
    Exited(ProgramId),
    Terminated(ProgramId),
}

struct DispatchExecutionContext<'a, S: Storage, RI: RuntimeInterface<S>> {
    program_id: ProgramId,
    code_id: CodeId,
    dispatch: Dispatch,
    block_config: &'a BlockConfig,
    program_context: &'a mut ProgramContext,
    receipts: &'a mut Vec<Receipt>,
    queue: &'a mut MessageQueue,
    waitlist: &'a mut Waitlist,
    gas_allowance: &'a mut u64,
    ri: &'a RI,
    _phantom: PhantomData<S>,
}

fn process_dispatch<S: Storage, RI: RuntimeInterface<S>>(
    ctx: &mut DispatchExecutionContext<S, RI>,
) -> Vec<JournalNote> {
    let program_context = match &ctx.program_context {
        ProgramContext::Executable(program_context) => program_context,
        ProgramContext::Exited(_) | ProgramContext::Terminated(_) => {
            todo!("Process dispatch for non-executable program")
        }
    };

    if program_context.status == InitStatus::Initialized && ctx.dispatch.kind == DispatchKind::Init
    {
        // Panic is impossible, because gear protocol does not provide functionality
        // to send second init message to any already existing program.
        unreachable!(
            "Init message {:?} is sent to already initialized program {:?}",
            ctx.dispatch.id, ctx.program_id,
        );
    }

    // If the destination program is uninitialized, then we allow
    // to process message, if it's a reply or init message.
    // Otherwise, we return error reply.
    if matches!(program_context.status, InitStatus::Uninitialized { message_id }
            if message_id != ctx.dispatch.id && ctx.dispatch.kind != DispatchKind::Reply)
    {
        if ctx.dispatch.kind == DispatchKind::Init {
            // Panic is impossible, because gear protocol does not provide functionality
            // to send second init message to any existing program.
            unreachable!(
                "Init message {:?} is not the first init message to the program {:?}",
                ctx.dispatch.id, ctx.program_id,
            );
        }

        todo!("Process handle messages to uninitialized program");
    }

    let payload = ctx.dispatch.payload_hash.with_hash_or_default(|hash| {
        ctx.ri
            .storage()
            .read_payload(hash)
            .expect("Cannot get payload")
    });

    let incoming_message = IncomingMessage::new(
        ctx.dispatch.id,
        ctx.dispatch.source,
        payload,
        ctx.dispatch.gas_limit,
        ctx.dispatch.value,
        ctx.dispatch.details,
    );

    let dispatch = IncomingDispatch::new(
        ctx.dispatch.kind,
        incoming_message,
        ctx.dispatch.context.take(),
    );

    let precharged_dispatch = core_processor::precharge_for_program(
        ctx.block_config,
        *ctx.gas_allowance,
        dispatch,
        ctx.program_id,
    )
    .expect("TODO: process precharge errors");

    let actor_data = ExecutableActorData {
        allocations: program_context.allocations.clone(),
        code_id: ctx.code_id,
        code_exports: program_context.code.exports().clone(),
        static_pages: program_context.code.static_pages(),
        gas_reservation_map: program_context.gas_reservation_map.clone(),
        memory_infix: program_context.memory_infix,
    };

    let context = core_processor::precharge_for_code_length(
        ctx.block_config,
        precharged_dispatch,
        ctx.program_id,
        actor_data,
    )
    .expect("TODO: process precharge errors");

    let context = ContextChargedForCode::from((context, program_context.code.code().len() as u32));
    let context = core_processor::precharge_for_memory(
        ctx.block_config,
        ContextChargedForInstrumentation::from(context),
    )
    .expect("TODO: process precharge errors");
    let execution_context = ProcessExecutionContext::from((
        context,
        program_context.code.clone(),
        program_context.balance,
    ));

    let random_data = ctx.ri.random_data();

    ctx.ri.init_lazy_pages(program_context.pages_map.clone());

    core_processor::process::<Ext<RI::LazyPages>>(ctx.block_config, execution_context, random_data)
        .unwrap_or_else(|err| unreachable!("{err}"))
}

fn prepare_executable_program_context<S: Storage>(
    program_id: ProgramId,
    balance: Value,
    state: ActiveProgram,
    instrumented_code: Option<InstrumentedCode>,
    ri: &impl RuntimeInterface<S>,
) -> ExecutableProgramContext {
    let code = instrumented_code.expect("Instrumented code must be provided if program is active");

    let allocations = state.allocations_hash.with_hash_or_default(|hash| {
        ri.storage()
            .read_allocations(hash)
            .expect("Cannot get allocations")
    });

    let gas_reservation_map = state.gas_reservation_map_hash.with_hash_or_default(|hash| {
        ri.storage()
            .read_gas_reservation_map(hash)
            .expect("Cannot get gas reservation map")
    });

    let pages_map = state.pages_hash.with_hash_or_default(|hash| {
        ri.storage()
            .read_pages(hash)
            .expect("Cannot get memory pages")
    });

    ExecutableProgramContext {
        allocations,
        code,
        gas_reservation_map,
        memory_infix: state.memory_infix,
        pages_map,
        status: state.status,
        balance,
    }
}

fn post_process_executable_program_context<S: Storage>(
    program_context: ExecutableProgramContext,
    queue_hash: MaybeHash,
    waitlist_hash: MaybeHash,
    ri: &impl RuntimeInterface<S>,
) -> ProgramState {
    let ExecutableProgramContext {
        allocations,
        code,
        gas_reservation_map,
        memory_infix,
        pages_map,
        balance,
        status,
    } = program_context;

    let allocations_hash = (allocations.intervals_amount() == 0)
        .then_some(MaybeHash::Empty)
        .unwrap_or_else(|| ri.storage().write_allocations(allocations).into());

    let pages_hash = pages_map
        .is_empty()
        .then_some(MaybeHash::Empty)
        .unwrap_or_else(|| ri.storage().write_pages(pages_map).into());

    let gas_reservation_map_hash = gas_reservation_map
        .is_empty()
        .then_some(MaybeHash::Empty)
        .unwrap_or_else(|| {
            ri.storage()
                .write_gas_reservation_map(gas_reservation_map)
                .into()
        });

    ProgramState {
        state: state::Program::Active(ActiveProgram {
            allocations_hash,
            pages_hash,
            gas_reservation_map_hash,
            memory_infix,
            status,
        }),
        queue_hash,
        waitlist_hash,
        balance,
    }
}

pub fn process_program<S: Storage>(
    program_id: ProgramId,
    program_state: ProgramState,
    instrumented_code: Option<InstrumentedCode>,
    max_messages_to_process: u32,
    gas_allowance: u64,
    code_id: CodeId,
    ri: &impl RuntimeInterface<S>,
) -> (ProgramState, Vec<Receipt>) {
    let block_info = ri.block_info();

    let mut waitlist = program_state.waitlist_hash.with_hash_or_default(|hash| {
        ri.storage()
            .read_waitlist(hash)
            .expect("Cannot get waitlist")
    });

    let mut dispatches_to_wake = Vec::new();
    let mut blocks_to_remove = Vec::new();
    for (block, list) in waitlist.range_mut(0..=block_info.height) {
        if list.is_empty() {
            unreachable!("Empty waitlist for block, must been removed from waitlist")
        }
        dispatches_to_wake.append(list);
        blocks_to_remove.push(*block);
    }

    for block in blocks_to_remove {
        waitlist.remove(&block);
    }

    let mut queue = program_state.queue_hash.with_hash_or_default(|hash| {
        ri.storage()
            .read_queue(hash)
            .expect("Cannot get message queue")
    });

    for dispatch in dispatches_to_wake {
        queue.push_back(dispatch);
    }

    if queue.is_empty() {
        // This is correct to return old state,
        // because waitlist and queue has not been changed, in case of empty queue.
        return (program_state, Vec::new());
    }

    // TODO: must be set by some runtime configuration
    let block_config = BlockConfig {
        block_info,
        performance_multiplier: Percent::new(100),
        forbidden_funcs: Default::default(),
        reserve_for: 125_000_000,
        gas_multiplier: GasMultiplier::from_gas_per_value(1), // TODO
        costs: Default::default(),                            // TODO
        existential_deposit: 0,                               // TODO
        mailbox_threshold: 3000,
        max_reservations: 50,
        max_pages: 512.into(),
        outgoing_limit: 1024,
        outgoing_bytes_limit: 64 * 1024 * 1024,
    };

    let mut program_context = match program_state.state {
        state::Program::Active(state) => {
            ProgramContext::Executable(prepare_executable_program_context(
                program_id,
                program_state.balance,
                state,
                instrumented_code,
                ri,
            ))
        }
        state::Program::Exited(program_id) => ProgramContext::Exited(program_id),
        state::Program::Terminated(program_id) => ProgramContext::Terminated(program_id),
    };

    let mut receipts = Vec::new();
    let mut messages_counter = 0;
    let mut gas_allowance = gas_allowance;
    while !queue.is_empty() && messages_counter < max_messages_to_process && gas_allowance > 0 {
        let dispatch = queue.pop_front().unwrap();
        let mut dispatch_context = DispatchExecutionContext {
            program_id,
            code_id,
            dispatch,
            block_config: &block_config,
            program_context: &mut program_context,
            receipts: &mut receipts,
            queue: &mut queue,
            waitlist: &mut waitlist,
            gas_allowance: &mut gas_allowance,
            ri,
            _phantom: PhantomData,
        };
        let journal = process_dispatch(&mut dispatch_context);
        core_processor::handle_journal(journal, &mut dispatch_context);
        messages_counter += 1;
    }

    let queue_hash = queue
        .is_empty()
        .then_some(MaybeHash::Empty)
        .unwrap_or_else(|| ri.storage().write_queue(queue).into());

    let waitlist_hash = waitlist
        .is_empty()
        .then_some(MaybeHash::Empty)
        .unwrap_or_else(|| ri.storage().write_waitlist(waitlist).into());

    let program_state = match program_context {
        ProgramContext::Executable(program_context) => {
            post_process_executable_program_context(program_context, queue_hash, waitlist_hash, ri)
        }
        ProgramContext::Exited(_) | ProgramContext::Terminated(_) => {
            todo!("Post process non-executable program context")
        }
    };

    (program_state, receipts)
}
