// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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
    builtin::BuiltinDispatcherFactory,
    manager::{CodeInfo, ExtManager, HandleKind},
    queue::StorageAccess,
    Config, CurrencyOf, MailboxOf, Pallet as Gear, ProgramStorageOf,
};
use common::{storage::*, CodeStorage, Origin, ProgramStorage};
use core::marker::PhantomData;
use core_processor::configs::BlockConfig;
use frame_support::traits::{Currency, Get};
use gear_core::{
    code::{Code, CodeAndId},
    ids::{CodeId, MessageId, ProgramId},
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
    assert!(gear_lazy_pages_interface::try_to_enable_lazy_pages(
        ProgramStorageOf::<T>::pages_final_prefix()
    ));

    // to see logs in bench tests
    #[cfg(feature = "std")]
    let _ = env_logger::try_init();

    let (builtins, _) = T::BuiltinDispatcherFactory::create();
    let ext_manager = ExtManager::<T>::new(builtins);
    let bn: u64 = Gear::<T>::block_number().unique_saturated_into();
    let root_message_id = MessageId::from(bn);

    let dispatch = match kind {
        HandleKind::Init(ref code) => {
            let program_id = ProgramId::generate_from_user(CodeId::generate(code), b"bench_salt");

            let schedule = T::Schedule::get();
            let code = Code::try_new(
                code.clone(),
                schedule.instruction_weights.version,
                |module| schedule.rules(module),
                schedule.limits.stack_height,
                schedule.limits.data_segments_amount.into(),
            )
            .map_err(|_| "Code failed to load")?;

            let code_and_id = CodeAndId::new(code);
            let code_info = CodeInfo::from_code_and_id(&code_and_id);

            let _ = Gear::<T>::set_code_with_metadata(code_and_id, source);

            ext_manager.set_program(
                program_id,
                &code_info,
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
            let program_id = ProgramId::generate_from_user(code_id, b"bench_salt");

            let code = T::CodeStorage::get_code(code_id).ok_or("Code not found in storage")?;
            let code_info = CodeInfo::from_code(&code_id, &code);

            ext_manager.set_program(
                program_id,
                &code_info,
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

    let storage = StorageAccess::<T>(PhantomData);
    let program_id = dispatch.destination();
    let dispatch = dispatch.into_stored().into_incoming(config.gas_limit);
    let balance = CurrencyOf::<T>::free_balance(&program_id.cast()).unique_saturated_into();
    let pallet_config = Gear::<T>::block_config();
    let block_config = BlockConfig {
        outgoing_limit: 2048,
        outgoing_bytes_limit: u32::MAX,
        max_pages: config.max_pages_override.unwrap_or(pallet_config.max_pages),
        ..pallet_config
    };

    let context = match core_processor::precharge(
        &storage,
        &block_config,
        config.gas_allowance,
        dispatch,
        program_id,
        balance,
    ) {
        Ok(ctx) => ctx,
        Err(_) => return Err("error in +_+_+"),
    };

    Ok(Exec {
        ext_manager,
        block_config,
        context,
        random_data: (vec![0u8; 32], 0),
    })
}
