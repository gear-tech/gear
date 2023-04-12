// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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
    manager::{CodeInfo, ExtManager, HandleKind},
    Config, CostsPerBlockOf, CurrencyOf, DbWeightOf, MailboxOf, Pallet as Gear, QueueOf,
};
use common::{scheduler::SchedulingCostsPerBlock, storage::*, CodeStorage, Origin};
use core_processor::{
    configs::{BlockConfig, BlockInfo},
    ContextChargedForCode, ContextChargedForInstrumentation,
};
use frame_support::traits::{Currency, Get};
use gear_core::{
    code::{Code, CodeAndId},
    ids::{CodeId, MessageId, ProgramId},
    message::{Dispatch, DispatchKind, Message, ReplyDetails, SignalDetails},
};
use sp_core::H256;
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{convert::TryInto, prelude::*};

#[cfg(feature = "lazy-pages")]
use crate::ProgramStorageOf;
#[cfg(feature = "lazy-pages")]
use common::ProgramStorage;

const DEFAULT_BLOCK_NUMBER: u32 = 0;

pub fn prepare_block_config<T>() -> BlockConfig
where
    T: Config,
    T::AccountId: Origin,
{
    let block_info = BlockInfo {
        height: Gear::<T>::block_number().unique_saturated_into(),
        timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
    };

    let existential_deposit = CurrencyOf::<T>::minimum_balance().unique_saturated_into();
    let mailbox_threshold = <T as Config>::MailboxThreshold::get();
    let waitlist_cost = CostsPerBlockOf::<T>::waitlist();
    let reserve_for = CostsPerBlockOf::<T>::reserve_for().unique_saturated_into();
    let reservation = CostsPerBlockOf::<T>::reservation().unique_saturated_into();

    let schedule = T::Schedule::get();

    BlockConfig {
        block_info,
        max_pages: T::Schedule::get().limits.memory_pages.into(),
        page_costs: T::Schedule::get().memory_weights.into(),
        existential_deposit,
        outgoing_limit: 2048,
        host_fn_weights: Default::default(),
        forbidden_funcs: Default::default(),
        mailbox_threshold,
        waitlist_cost,
        dispatch_hold_cost: CostsPerBlockOf::<T>::dispatch_stash(),
        reserve_for,
        reservation,
        read_cost: DbWeightOf::<T>::get().reads(1).ref_time(),
        write_cost: DbWeightOf::<T>::get().writes(1).ref_time(),
        write_per_byte_cost: schedule.db_write_per_byte.ref_time(),
        read_per_byte_cost: schedule.db_read_per_byte.ref_time(),
        module_instantiation_byte_cost: schedule.module_instantiation_per_byte.ref_time(),
        max_reservations: T::ReservationsLimit::get(),
        code_instrumentation_cost: schedule.code_instrumentation_cost.ref_time(),
        code_instrumentation_byte_cost: schedule.code_instrumentation_byte_cost.ref_time(),
    }
}

pub struct PrepareConfig {
    pub value: u128,
    pub gas_allowance: u64,
    pub gas_limit: u64,
}

impl Default for PrepareConfig {
    fn default() -> Self {
        PrepareConfig {
            value: 0,
            gas_allowance: u64::MAX,
            gas_limit: u64::MAX / 2,
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
    #[cfg(feature = "lazy-pages")]
    assert!(gear_lazy_pages_common::try_to_enable_lazy_pages(
        ProgramStorageOf::<T>::pages_final_prefix()
    ));

    // to see logs in bench tests
    #[cfg(feature = "std")]
    let _ = env_logger::try_init();

    let ext_manager = ExtManager::<T>::default();
    let bn: u64 = Gear::<T>::block_number().unique_saturated_into();
    let root_message_id = MessageId::from(bn);

    let dispatch = match kind {
        HandleKind::Init(ref code) => {
            let program_id = ProgramId::generate(CodeId::generate(code), b"bench_salt");

            let schedule = T::Schedule::get();
            let code = Code::try_new(
                code.clone(),
                schedule.instruction_weights.version,
                |module| schedule.rules(module),
                schedule.limits.stack_height,
            )
            .map_err(|_| "Code failed to load")?;

            let code_and_id = CodeAndId::new(code);
            let code_info = CodeInfo::from_code_and_id(&code_and_id);

            let _ = Gear::<T>::set_code_with_metadata(code_and_id, source);

            ExtManager::<T>::default().set_program(
                program_id,
                &code_info,
                root_message_id,
                DEFAULT_BLOCK_NUMBER.into(),
            );

            Dispatch::new(
                DispatchKind::Init,
                Message::new(
                    root_message_id,
                    ProgramId::from_origin(source),
                    program_id,
                    payload.try_into()?,
                    Some(u64::MAX),
                    config.value,
                    None,
                ),
            )
        }
        HandleKind::InitByHash(code_id) => {
            let program_id = ProgramId::generate(code_id, b"bench_salt");

            let code = T::CodeStorage::get_code(code_id).ok_or("Code not found in storage")?;
            let code_info = CodeInfo::from_code(&code_id, &code);

            ExtManager::<T>::default().set_program(
                program_id,
                &code_info,
                root_message_id,
                DEFAULT_BLOCK_NUMBER.into(),
            );

            Dispatch::new(
                DispatchKind::Init,
                Message::new(
                    root_message_id,
                    ProgramId::from_origin(source),
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
                ProgramId::from_origin(source),
                dest,
                payload.try_into()?,
                Some(u64::MAX),
                config.value,
                None,
            ),
        ),
        HandleKind::Reply(msg_id, exit_code) => {
            let (msg, _bn) =
                MailboxOf::<T>::remove(<T::AccountId as Origin>::from_origin(source), msg_id)
                    .map_err(|_| "Internal error: unable to find message in mailbox")?;
            Dispatch::new(
                DispatchKind::Reply,
                Message::new(
                    root_message_id,
                    ProgramId::from_origin(source),
                    msg.source(),
                    payload.try_into()?,
                    Some(u64::MAX),
                    config.value,
                    Some(ReplyDetails::new(msg.id(), exit_code).into()),
                ),
            )
        }
        HandleKind::Signal(msg_id, status_code) => {
            let (msg, _bn) =
                MailboxOf::<T>::remove(<T::AccountId as Origin>::from_origin(source), msg_id)
                    .map_err(|_| "Internal error: unable to find message in mailbox")?;
            Dispatch::new(
                DispatchKind::Signal,
                Message::new(
                    root_message_id,
                    ProgramId::from_origin(source),
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
    let actor = ext_manager
        .get_actor(actor_id)
        .ok_or("Program not found in the storage")?;

    let block_config = prepare_block_config::<T>();

    let precharged_dispatch = core_processor::precharge_for_program(
        &block_config,
        config.gas_allowance,
        queued_dispatch.into_incoming(config.gas_limit),
        actor_id,
    )
    .map_err(|_| "core_processor::precharge_for_program failed")?;

    let balance = actor.balance;
    let context = core_processor::precharge_for_code_length(
        &block_config,
        precharged_dispatch,
        actor_id,
        actor.executable_data,
    )
    .map_err(|_| "core_processor::precharge_for_code failed")?;

    let code =
        T::CodeStorage::get_code(context.actor_data().code_id).ok_or("Program code not found")?;

    let context = ContextChargedForCode::from((context, code.code().len() as u32));
    let context = core_processor::precharge_for_memory(
        &block_config,
        ContextChargedForInstrumentation::from(context),
    )
    .map_err(|_| "core_processor::precharge_for_memory failed")?;

    let origin = ProgramId::from_origin(source);

    Ok(Exec {
        ext_manager,
        block_config,
        context: (context, code, balance, origin).into(),
        random_data: (vec![0u8; 32], 0),
        memory_pages: Default::default(),
    })
}
