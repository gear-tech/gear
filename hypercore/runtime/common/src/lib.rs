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
use state::{
    ActiveProgram, Dispatch, HashAndLen, InitStatus, MaybeHash, MessageQueue, ProgramState,
    Storage, Waitlist,
};

extern crate alloc;

mod journal;
pub mod state;

pub use journal::Handler;

const RUNTIME_ID: u32 = 0;

pub trait RuntimeInterface<S: Storage> {
    type LazyPages: LazyPagesInterface + 'static;

    fn block_info(&self) -> BlockInfo;
    fn init_lazy_pages(&self, pages_map: BTreeMap<GearPage, H256>);
    fn random_data(&self) -> (Vec<u8>, u32);
    fn storage(&self) -> &S;
}

pub fn process_next_message<S: Storage, RI: RuntimeInterface<S>>(
    program_id: ProgramId,
    program_state: ProgramState,
    instrumented_code: Option<InstrumentedCode>,
    code_id: CodeId,
    ri: &RI,
) -> Vec<JournalNote> {
    let block_info = ri.block_info();

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

    let active_state = match program_state.state {
        state::Program::Active(state) => state,
        state::Program::Exited(program_id) | state::Program::Terminated(program_id) => {
            log::trace!("Program {program_id} is not active");
            todo!("Process non-active program")
        }
    };

    let Dispatch {
        id: dispatch_id,
        kind,
        source,
        payload_hash,
        gas_limit,
        value,
        details,
        context,
    } = queue.pop_front().unwrap();

    if active_state.status == InitStatus::Initialized && kind == DispatchKind::Init {
        // Panic is impossible, because gear protocol does not provide functionality
        // to send second init message to any already existing program.
        unreachable!(
            "Init message {dispatch_id} is sent to already initialized program {program_id}",
        );
    }

    // If the destination program is uninitialized, then we allow
    // to process message, if it's a reply or init message.
    // Otherwise, we return error reply.
    if matches!(active_state.status, InitStatus::Uninitialized { message_id }
            if message_id != dispatch_id && kind != DispatchKind::Reply)
    {
        if kind == DispatchKind::Init {
            // Panic is impossible, because gear protocol does not provide functionality
            // to send second init message to any existing program.
            unreachable!(
                "Init message {} is not the first init message to the program {}",
                dispatch_id, program_id,
            );
        }

        todo!("Process messages to uninitialized program");
    }

    let payload = payload_hash
        .with_hash_or_default(|hash| ri.storage().read_payload(hash).expect("Cannot get payload"));

    let incoming_message =
        IncomingMessage::new(dispatch_id, source, payload, gas_limit, value, details);

    let dispatch = IncomingDispatch::new(kind, incoming_message, context);

    let precharged_dispatch = core_processor::precharge_for_program(
        &block_config,
        1_000_000_000_000,
        dispatch,
        program_id,
    )
    .expect("TODO: process precharge errors");

    let code = instrumented_code.expect("Instrumented code must be provided if program is active");

    let allocations = active_state.allocations_hash.with_hash_or_default(|hash| {
        ri.storage()
            .read_allocations(hash)
            .expect("Cannot get allocations")
    });

    let gas_reservation_map = active_state
        .gas_reservation_map_hash
        .with_hash_or_default(|hash| {
            ri.storage()
                .read_gas_reservation_map(hash)
                .expect("Cannot get gas reservation map")
        });

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
        gas_reservation_map,
        memory_infix: active_state.memory_infix,
    };

    let context = core_processor::precharge_for_code_length(
        &block_config,
        precharged_dispatch,
        program_id,
        actor_data,
    )
    .expect("TODO: process precharge errors");

    let context = ContextChargedForCode::from((context, code.code().len() as u32));
    let context = core_processor::precharge_for_memory(
        &block_config,
        ContextChargedForInstrumentation::from(context),
    )
    .expect("TODO: process precharge errors");

    let execution_context = ProcessExecutionContext::from((context, code, program_state.balance));

    let random_data = ri.random_data();

    ri.init_lazy_pages(pages_map.clone());

    core_processor::process::<Ext<RI::LazyPages>>(&block_config, execution_context, random_data)
        .unwrap_or_else(|err| unreachable!("{err}"))
}
