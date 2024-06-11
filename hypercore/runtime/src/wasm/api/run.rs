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

use alloc::vec::Vec;
use core_processor::{
    common::{ExecutableActorData, JournalNote},
    configs::BlockConfig,
};
use gear_core::{
    code::InstrumentedCode,
    message::{DispatchKind, StoredDispatch, StoredMessage},
    pages::{numerated::tree::IntervalsTree, GearPage, WasmPage, WasmPagesAmount},
};
use gear_core_backend::env::Environment;
use gear_lazy_pages_interface::{LazyPagesInterface, LazyPagesRuntimeInterface};
use gear_sandbox::{
    default_executor::{Caller, EnvironmentDefinitionBuilder, Instance, Memory, Store},
    HostError, ReturnValue, SandboxEnvironmentBuilder, SandboxInstance, SandboxMemory,
    SandboxStore, Value,
};
use gear_sandbox_env::WasmReturnValue;
use gprimitives::H256;
use gsys::{GasMultiplier, Percent};
use parity_scale_codec::Encode;

pub fn run(code: InstrumentedCode) {
    log::info!("You're calling 'run(..)'");

    assert!(LazyPagesRuntimeInterface::try_to_enable_lazy_pages(
        Default::default()
    ));

    for note in proc(code, Default::default()) {
        log::debug!("{note:?}");
        if let JournalNote::SendDispatch { dispatch, .. } = note {
            let Some(reply_details) = dispatch.reply_details() else {
                continue;
            };
            if reply_details.to_message_id().is_zero() {
                assert_eq!(dispatch.payload_bytes(), b"PONG");
            }
        }
    }
}

pub fn proc(code: InstrumentedCode, allocations: IntervalsTree<WasmPage>) -> Vec<JournalNote> {
    let block_config = BlockConfig {
        block_info: Default::default(),
        performance_multiplier: Percent::new(100),
        forbidden_funcs: Default::default(),
        reserve_for: 0,
        gas_multiplier: GasMultiplier::from_value_per_gas(1),
        costs: Default::default(),
        existential_deposit: 0,
        mailbox_threshold: 0,
        max_reservations: 0,
        max_pages: 256.into(),
        outgoing_limit: u32::MAX,
        outgoing_bytes_limit: u32::MAX,
    };

    let gas_limit = u64::MAX;

    let kind = DispatchKind::Handle;

    let id = Default::default();
    let source = Default::default();
    let destination = Default::default();
    let payload = b"PING".to_vec().try_into().unwrap();
    let value = 0;
    let details = None;

    let message = StoredMessage::new(id, source, destination, payload, value, details);

    let context = None;

    let dispatch = StoredDispatch::new(kind, message, context);

    let gas_allowance = u64::MAX;

    let precharged_dispatch = match core_processor::precharge_for_program(
        &block_config,
        gas_allowance,
        dispatch.into_incoming(gas_limit),
        destination,
    ) {
        Ok(dispatch) => dispatch,
        Err(journal) => return journal,
    };

    let actor_data = ExecutableActorData {
        allocations,
        memory_infix: Default::default(),
        code_id: Default::default(),
        code_exports: code.exports().clone(),
        static_pages: code.static_pages(),
        gas_reservation_map: Default::default(),
    };

    let context = match core_processor::precharge_for_code_length(
        &block_config,
        precharged_dispatch,
        destination,
        actor_data,
    ) {
        Ok(dispatch) => dispatch,
        Err(journal) => return journal,
    };

    let code_len_bytes = 0;

    let context = match core_processor::precharge_for_code(&block_config, context, code_len_bytes) {
        Ok(dispatch) => dispatch,
        Err(journal) => return journal,
    };

    let original_code_len = 0;

    let context = match core_processor::precharge_for_instrumentation(
        &block_config,
        context,
        original_code_len,
    ) {
        Ok(context) => context,
        Err(journal) => return journal,
    };

    let context = match core_processor::precharge_for_memory(&block_config, context) {
        Ok(dispatch) => dispatch,
        Err(journal) => return journal,
    };

    let random = H256::default();
    let bn = 0;

    let balance = 0;

    core_processor::process::<core_processor::Ext<LazyPagesRuntimeInterface>>(
        &block_config,
        (context, code, balance).into(),
        (random.encode(), bn),
    )
    .unwrap_or_else(|e| unreachable!("{e}"))
}
