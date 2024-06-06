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

use alloc::{collections::BTreeMap, vec::Vec};
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
use state::{Dispatch, MessageQueue, ProgramState};

extern crate alloc;

pub mod state;

pub trait CASReader {
    fn read(&self, hash: &H256) -> Option<Vec<u8>>;
}

pub trait RuntimeInterface: CASReader {
    type LazyPages: LazyPagesInterface + 'static;

    fn block_info(&self) -> BlockInfo;
    fn init_lazy_pages(&self, pages_map: BTreeMap<GearPage, H256>);
    fn random_data(&self) -> (Vec<u8>, u32);
}

#[allow(clippy::too_many_arguments)]
fn process_dispatch<RI: RuntimeInterface>(
    program_id: ProgramId,
    block_config: &BlockConfig,
    allocations: IntervalsTree<WasmPage>,
    code: InstrumentedCode,
    gas_reservation_map: GasReservationMap,
    dispatch: Dispatch,
    code_id: CodeId,
    memory_infix: MemoryInfix,
    pages_map: BTreeMap<GearPage, H256>,
    balance: Value,
    ri: &mut RI,
) -> Vec<JournalNote> {
    let payload = dispatch.payload_hash.read(ri).unwrap_or_default();
    let incoming_message = IncomingMessage::new(
        dispatch.id,
        dispatch.source,
        payload,
        dispatch.gas_limit,
        dispatch.value,
        dispatch.details,
    );
    let dispatch = IncomingDispatch::new(dispatch.kind, incoming_message, dispatch.context);

    let precharged_dispatch = core_processor::precharge_for_program(
        block_config,
        1_000_000_000_000, // TODO
        dispatch,
        program_id,
    )
    .expect("TODO: process precharge errors");

    let actor_data = ExecutableActorData {
        allocations,
        code_id,
        code_exports: code.exports().clone(),
        static_pages: code.static_pages(),
        gas_reservation_map,
        memory_infix,
    };

    let context = core_processor::precharge_for_code_length(
        block_config,
        precharged_dispatch,
        program_id,
        actor_data,
    )
    .expect("TODO: process precharge errors");

    let context = ContextChargedForCode::from((context, code.code().len() as u32));
    let context = core_processor::precharge_for_memory(
        block_config,
        ContextChargedForInstrumentation::from(context),
    )
    .expect("TODO: process precharge errors");
    let execution_context = ProcessExecutionContext::from((context, code, balance));

    let random_data = ri.random_data();

    ri.init_lazy_pages(pages_map);

    core_processor::process::<Ext<RI::LazyPages>>(block_config, execution_context, random_data)
        .unwrap()
}

pub fn process_program(
    program_id: ProgramId,
    program_state: &ProgramState,
    ri: &mut impl RuntimeInterface,
) -> Vec<JournalNote> {
    let mut queue: MessageQueue = program_state.queue_hash.read(ri).unwrap_or_default();

    if queue.0.is_empty() {
        return Vec::new();
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

    let mut journal = Vec::new();
    while let Some(dispatch) = queue.0.pop() {
        let j = process_dispatch(
            program_id,
            &block_config,
            allocations.clone(),
            code.clone(),
            gas_reservation_map.clone(),
            dispatch,
            code_id,
            program_state.memory_infix,
            pages_map.clone(),
            program_state.balance,
            ri,
        );
        journal.extend(j);

        // TODO: handle journal and store receipts
    }

    return journal;
}
