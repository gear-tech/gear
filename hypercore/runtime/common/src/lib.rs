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

use alloc::{collections::BTreeMap, vec::Vec};
use core::mem::swap;
use core_processor::{
    common::{ExecutableActorData, JournalNote},
    configs::{BlockConfig, BlockInfo},
    ContextChargedForCode, ContextChargedForInstrumentation, Ext, ProcessExecutionContext,
};
use gear_core::{
    code::InstrumentedCode,
    ids::ProgramId,
    message::{IncomingDispatch, IncomingMessage, Value},
    pages::{numerated::tree::IntervalsTree, GearPage, WasmPage},
    program::MemoryInfix,
    reservation::GasReservationMap,
};
use gear_lazy_pages_common::LazyPagesInterface;
use gprimitives::{CodeId, H256};
use gsys::{GasMultiplier, Percent};
use parity_scale_codec::{Decode, Encode};
use receipts::Receipt;
use state::{Dispatch, MaybeHash, MessageQueue, ProgramState};

extern crate alloc;

mod journal;
pub mod receipts;
pub mod state;

pub trait CASReader {
    fn read(&self, hash: &H256) -> Option<Vec<u8>>;
}

pub trait CASWriter {
    fn write(&mut self, data: &[u8]) -> H256;
}

pub trait RuntimeInterface: CASReader + CASWriter {
    type LazyPages: LazyPagesInterface + 'static;

    fn block_info(&self) -> BlockInfo;
    fn init_lazy_pages(&self, pages_map: BTreeMap<GearPage, H256>);
    fn random_data(&self) -> (Vec<u8>, u32);
}

struct ProgramContext {
    program_id: ProgramId,
    allocations: IntervalsTree<WasmPage>,
    code: InstrumentedCode,
    gas_reservation_map: GasReservationMap,
    code_id: CodeId,
    memory_infix: MemoryInfix,
    pages_map: BTreeMap<GearPage, H256>,
    balance: Value,
    receipts: Vec<Receipt>,
}

struct DispatchExecutionContext<'a, RI: RuntimeInterface> {
    program_context: &'a mut ProgramContext,
    dispatch: Dispatch,
    ri: &'a mut RI,
}

fn process_dispatch<RI: RuntimeInterface>(
    block_config: &BlockConfig,
    ctx: &mut DispatchExecutionContext<RI>,
) -> Vec<JournalNote> {
    let payload = ctx.dispatch.payload_hash.read(ctx.ri).unwrap_or_default();
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
        ctx.dispatch.context.take(), // TODO: do not forget to set it back in wait
    );

    let precharged_dispatch = core_processor::precharge_for_program(
        block_config,
        1_000_000_000_000, // TODO
        dispatch,
        ctx.program_context.program_id,
    )
    .expect("TODO: process precharge errors");

    let actor_data = ExecutableActorData {
        allocations: ctx.program_context.allocations.clone(),
        code_id: ctx.program_context.code_id,
        code_exports: ctx.program_context.code.exports().clone(),
        static_pages: ctx.program_context.code.static_pages(),
        gas_reservation_map: ctx.program_context.gas_reservation_map.clone(),
        memory_infix: ctx.program_context.memory_infix,
    };

    let context = core_processor::precharge_for_code_length(
        block_config,
        precharged_dispatch,
        ctx.program_context.program_id,
        actor_data,
    )
    .expect("TODO: process precharge errors");

    let context =
        ContextChargedForCode::from((context, ctx.program_context.code.code().len() as u32));
    let context = core_processor::precharge_for_memory(
        block_config,
        ContextChargedForInstrumentation::from(context),
    )
    .expect("TODO: process precharge errors");
    let execution_context = ProcessExecutionContext::from((
        context,
        ctx.program_context.code.clone(),
        ctx.program_context.balance,
    ));

    let random_data = ctx.ri.random_data();

    ctx.ri
        .init_lazy_pages(ctx.program_context.pages_map.clone());

    core_processor::process::<Ext<RI::LazyPages>>(block_config, execution_context, random_data)
        .unwrap_or_else(|err| unreachable!("{err}"))
}

pub fn process_program(
    program_id: ProgramId,
    program_state: ProgramState,
    ri: &mut impl RuntimeInterface,
) -> (ProgramState, Vec<Receipt>) {
    let mut queue: MessageQueue = program_state.queue_hash.read(ri).unwrap_or_default();

    if queue.0.is_empty() {
        return (program_state, Vec::new());
    }

    // TODO: must be set by some runtime configuration
    let block_config = BlockConfig {
        block_info: ri.block_info(),
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

    let code_id: CodeId = program_state.original_code_hash.hash.into();

    let code: InstrumentedCode = program_state.instrumented_code_hash.read(ri);
    let allocations: IntervalsTree<WasmPage> =
        program_state.allocations_hash.read(ri).unwrap_or_default();
    let gas_reservation_map: GasReservationMap = program_state
        .gas_reservation_map_hash
        .read(ri)
        .unwrap_or_default();
    let pages_map: BTreeMap<GearPage, H256> = program_state.pages_hash.read(ri).unwrap_or_default();

    let mut receipts = Vec::new();
    let mut program_context = ProgramContext {
        program_id,
        allocations,
        code,
        gas_reservation_map,
        code_id,
        memory_infix: program_state.memory_infix,
        pages_map,
        balance: program_state.balance,
        receipts: Vec::new(),
    };

    while let Some(dispatch) = queue.0.pop() {
        let mut dispatch_context = DispatchExecutionContext {
            program_context: &mut program_context,
            dispatch,
            ri,
        };
        let journal = process_dispatch(&block_config, &mut dispatch_context);
        core_processor::handle_journal(journal, &mut dispatch_context);
        receipts.append(&mut dispatch_context.program_context.receipts);
    }

    let queue_hash = queue
        .0
        .is_empty()
        .then_some(MaybeHash::Empty)
        .unwrap_or_else(|| ri.write(&queue.encode()).into());
    let allocations_hash = (program_context.allocations.intervals_amount() == 0)
        .then_some(MaybeHash::Empty)
        .unwrap_or_else(|| ri.write(&program_context.allocations.encode()).into());
    let pages_hash = program_context
        .pages_map
        .is_empty()
        .then_some(MaybeHash::Empty)
        .unwrap_or_else(|| ri.write(&program_context.pages_map.encode()).into());
    let gas_reservation_map_hash = program_context
        .gas_reservation_map
        .is_empty()
        .then_some(MaybeHash::Empty)
        .unwrap_or_else(|| {
            ri.write(&program_context.gas_reservation_map.encode())
                .into()
        });

    let program_state = ProgramState {
        queue_hash,
        allocations_hash,
        pages_hash,
        gas_reservation_map_hash,
        balance: program_context.balance,
        original_code_hash: program_state.original_code_hash,
        instrumented_code_hash: program_state.instrumented_code_hash,
        memory_infix: program_state.memory_infix,
    };

    (program_state, receipts)
}
