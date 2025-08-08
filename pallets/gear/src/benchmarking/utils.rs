// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Utils for benchmarks.

use super::Exec;
use crate::{
    Config, CurrencyOf, LazyPagesInterface, LazyPagesRuntimeInterface, MailboxOf, Pallet as Gear,
    ProgramStorageOf, QueueOf,
    builtin::BuiltinDispatcherFactory,
    manager::{ExtManager, HandleKind},
};
use common::{CodeStorage, Origin, Program, ProgramStorage, storage::*};
use core_processor::{
    ProcessExecutionContext, common::ExecutableActorData, configs::BlockConfig,
    precharge::ContextCharged,
};
use frame_support::traits::{Currency, Get};
use gear_core::{
    code::{Code, CodeAndId, InstrumentedCodeAndMetadata},
    ids::{ActorId, CodeId, MessageId, prelude::*},
    message::{Dispatch, DispatchKind, Message, ReplyDetails, SignalDetails},
    pages::WasmPagesAmount,
};
use sp_core::H256;
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{convert::TryInto, prelude::*};

const DEFAULT_BLOCK_NUMBER: u32 = 0;
const DEFAULT_INTERVAL: u32 = 1_000;

pub struct PrepareConfig {
    pub value: u128,
    pub gas_allowance: u64,
    pub gas_limit: u64,
    pub max_pages_override: Option<WasmPagesAmount>,
}

impl Default for PrepareConfig {
    fn default() -> Self {
        PrepareConfig {
            value: 0,
            gas_allowance: u64::MAX,
            gas_limit: u64::MAX / 2,
            max_pages_override: None,
        }
    }
}

pub fn prepare_exec<T>(
    source: H256,
    kind: HandleKind,
    payload: Vec<u8>,
    config: PrepareConfig,
) -> Result<Exec<T>, &'static str>
where
    T: Config,
    T::AccountId: Origin,
{
    let prefix = ProgramStorageOf::<T>::pages_final_prefix();
    assert!(LazyPagesRuntimeInterface::try_to_enable_lazy_pages(prefix));

    // to see logs in bench tests
    #[cfg(feature = "std")]
    let _ = tracing_subscriber::fmt::try_init();

    let (builtins, _) = T::BuiltinDispatcherFactory::create();
    let ext_manager = ExtManager::<T>::new(builtins);
    let bn: u64 = Gear::<T>::block_number().unique_saturated_into();
    let root_message_id = MessageId::from(bn);

    let dispatch = match kind {
        HandleKind::Init(ref code) => {
            let program_id = ActorId::generate_from_user(CodeId::generate(code), b"bench_salt");

            let schedule = T::Schedule::get();
            let code = Code::try_new(
                code.clone(),
                schedule.instruction_weights.version,
                |module| schedule.rules(module),
                schedule.limits.stack_height,
                schedule.limits.data_segments_amount.into(),
                schedule.limits.type_section_len.into(),
                schedule.limits.type_section_params_per_type.into(),
            )
            .map_err(|_| "Code failed to load")?;

            let code_and_id = CodeAndId::new(code);
            let code_id = code_and_id.code_id();

            let _ = Gear::<T>::set_code(code_and_id);

            ext_manager.set_program(
                program_id,
                code_id,
                root_message_id,
                DEFAULT_BLOCK_NUMBER.saturating_add(DEFAULT_INTERVAL).into(),
            );

            Dispatch::new(
                DispatchKind::Init,
                Message::new(
                    root_message_id,
                    source.cast(),
                    program_id,
                    payload.try_into()?,
                    Some(u64::MAX),
                    config.value,
                    None,
                ),
            )
        }
        HandleKind::InitByHash(code_id) => {
            let program_id = ActorId::generate_from_user(code_id, b"bench_salt");

            ext_manager.set_program(
                program_id,
                code_id,
                root_message_id,
                DEFAULT_BLOCK_NUMBER.saturating_add(DEFAULT_INTERVAL).into(),
            );

            Dispatch::new(
                DispatchKind::Init,
                Message::new(
                    root_message_id,
                    source.cast(),
                    program_id,
                    payload.try_into()?,
                    Some(u64::MAX),
                    config.value,
                    None,
                ),
            )
        }
        HandleKind::Handle(dest) => Dispatch::new(
            DispatchKind::Handle,
            Message::new(
                root_message_id,
                source.cast(),
                dest,
                payload.try_into()?,
                Some(u64::MAX),
                config.value,
                None,
            ),
        ),
        HandleKind::Reply(msg_id, exit_code) => {
            let (msg, _bn) = MailboxOf::<T>::remove(source.cast(), msg_id)
                .map_err(|_| "Internal error: unable to find message in mailbox")?;
            Dispatch::new(
                DispatchKind::Reply,
                Message::new(
                    root_message_id,
                    source.cast(),
                    msg.source(),
                    payload.try_into()?,
                    Some(u64::MAX),
                    config.value,
                    Some(ReplyDetails::new(msg.id(), exit_code).into()),
                ),
            )
        }
        HandleKind::Signal(msg_id, status_code) => {
            let (msg, _bn) = MailboxOf::<T>::remove(source.cast(), msg_id)
                .map_err(|_| "Internal error: unable to find message in mailbox")?;
            Dispatch::new(
                DispatchKind::Signal,
                Message::new(
                    root_message_id,
                    source.cast(),
                    msg.source(),
                    payload.try_into()?,
                    Some(u64::MAX),
                    config.value,
                    Some(SignalDetails::new(msg.id(), status_code).into()),
                ),
            )
        }
    };

    let dispatch = dispatch.into_stored();

    QueueOf::<T>::clear();

    QueueOf::<T>::queue(dispatch).map_err(|_| "Messages storage corrupted")?;

    let queued_dispatch = match QueueOf::<T>::dequeue().map_err(|_| "MQ storage corrupted")? {
        Some(d) => d,
        None => return Err("Dispatch not found"),
    };

    let actor_id = queued_dispatch.destination();

    let pallet_config = Gear::<T>::block_config();
    let block_config = BlockConfig {
        outgoing_limit: 2048,
        outgoing_bytes_limit: u32::MAX,
        max_pages: config.max_pages_override.unwrap_or(pallet_config.max_pages),
        ..pallet_config
    };

    let context = ContextCharged::new(
        actor_id,
        queued_dispatch.into_incoming(config.gas_limit),
        config.gas_allowance,
    );

    let context = context
        .charge_for_program(&block_config)
        .map_err(|_| "core_processor::precharge_for_program failed")?;

    let active = match ProgramStorageOf::<T>::get_program(actor_id) {
        Some(Program::Active(active)) => active,
        _ => return Err("Program not found"),
    };

    let balance: u128 = CurrencyOf::<T>::free_balance(&actor_id.cast()).unique_saturated_into();

    let context = context
        .charge_for_code_metadata(&block_config)
        .map_err(|_| "core_processor::precharge_for_code_metadata failed")?;

    let code_metadata =
        T::CodeStorage::get_code_metadata(active.code_id).ok_or("Code metadata not found")?;

    let context = context
        .charge_for_instrumented_code(
            &block_config,
            code_metadata
                .instrumented_code_len()
                .ok_or("Failed to get instrumented code length")?,
        )
        .map_err(|_| "core_processor::precharge_for_instrumented_code failed")?;

    let code =
        T::CodeStorage::get_instrumented_code(active.code_id).ok_or("Program code not found")?;

    let context = context
        .charge_for_allocations(&block_config, active.allocations_tree_len)
        .map_err(|_| "core_processor::precharge_for_allocations failed")?;

    let allocations = ProgramStorageOf::<T>::allocations(actor_id).unwrap_or_default();

    let actor_data = ExecutableActorData {
        allocations,
        gas_reservation_map: active.gas_reservation_map,
        memory_infix: active.memory_infix,
    };

    let context = context
        .charge_for_module_instantiation(
            &block_config,
            actor_data,
            code.instantiated_section_sizes(),
            &code_metadata,
        )
        .map_err(|_| "core_processor::precharge_for_module_instantiation failed")?;

    let process_exec_context = ProcessExecutionContext::new(
        context,
        InstrumentedCodeAndMetadata {
            instrumented_code: code,
            metadata: code_metadata,
        },
        balance,
    );

    Ok(Exec {
        ext_manager,
        block_config,
        context: process_exec_context,
        random_data: (vec![0u8; 32], 0),
    })
}
