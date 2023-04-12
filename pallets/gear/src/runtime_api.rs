// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use super::*;
use common::ActiveProgram;
use core::convert::TryFrom;
use gear_core::memory::WasmPage;
use gear_wasm_instrument::syscalls::SysCallName;

pub(crate) struct CodeWithMemoryData {
    pub instrumented_code: InstrumentedCode,
    pub allocations: BTreeSet<WasmPage>,
    pub program_pages: Option<BTreeMap<GearPage, PageBuf>>,
}

impl<T: Config> Pallet<T>
where
    T::AccountId: Origin,
{
    pub(crate) fn calculate_gas_info_impl(
        source: H256,
        kind: HandleKind,
        initial_gas: u64,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        allow_skip_zero_replies: bool,
    ) -> Result<GasInfo, Vec<u8>> {
        let account = <T::AccountId as Origin>::from_origin(source);

        let balance = CurrencyOf::<T>::free_balance(&account);
        let max_balance: BalanceOf<T> =
            T::GasPrice::gas_price(initial_gas) + value.unique_saturated_into();
        CurrencyOf::<T>::deposit_creating(&account, max_balance.saturating_sub(balance));

        let who = frame_support::dispatch::RawOrigin::Signed(account);
        let value: BalanceOf<T> = value.unique_saturated_into();

        QueueOf::<T>::clear();

        match kind {
            HandleKind::Init(code) => {
                let salt = b"calculate_gas_salt".to_vec();
                Self::upload_program(who.into(), code, salt, payload, initial_gas, value).map_err(
                    |e| format!("Internal error: upload_program failed with '{e:?}'").into_bytes(),
                )?;
            }
            HandleKind::InitByHash(code_id) => {
                let salt = b"calculate_gas_salt".to_vec();
                Self::create_program(who.into(), code_id, salt, payload, initial_gas, value)
                    .map_err(|e| {
                        format!("Internal error: create_program failed with '{e:?}'").into_bytes()
                    })?;
            }
            HandleKind::Handle(destination) => {
                Self::send_message(who.into(), destination, payload, initial_gas, value).map_err(
                    |e| format!("Internal error: send_message failed with '{e:?}'").into_bytes(),
                )?;
            }
            HandleKind::Reply(reply_to_id, _status_code) => {
                Self::send_reply(who.into(), reply_to_id, payload, initial_gas, value).map_err(
                    |e| format!("Internal error: send_reply failed with '{e:?}'").into_bytes(),
                )?;
            }
            HandleKind::Signal(_signal_from, _status_code) => {
                return Err("Gas calculation for `handle_signal` is not supported"
                    .as_bytes()
                    .to_vec());
            }
        };

        let (main_message_id, main_program_id) = QueueOf::<T>::iter()
            .next()
            .ok_or_else(|| b"Internal error: failed to get last message".to_vec())
            .and_then(|queued| {
                queued
                    .map_err(|_| b"Internal error: failed to retrieve queued dispatch".to_vec())
                    .map(|dispatch| (dispatch.id(), dispatch.destination()))
            })?;

        let mut block_config = Self::block_config();
        block_config.forbidden_funcs = [SysCallName::GasAvailable].into();

        #[cfg(feature = "lazy-pages")]
        let lazy_pages_enabled = {
            let prefix = ProgramStorageOf::<T>::pages_final_prefix();
            if !lazy_pages::try_to_enable_lazy_pages(prefix) {
                unreachable!("By some reasons we cannot run lazy-pages on this machine");
            }
            true
        };

        #[cfg(not(feature = "lazy-pages"))]
        let lazy_pages_enabled = false;

        let mut min_limit = 0;
        let mut reserved = 0;
        let mut burned = 0;

        let mut ext_manager = ExtManager::<T>::default();

        while let Some(queued_dispatch) =
            QueueOf::<T>::dequeue().map_err(|_| b"MQ storage corrupted".to_vec())?
        {
            let actor_id = queued_dispatch.destination();

            let actor = ext_manager
                .get_actor(actor_id)
                .ok_or_else(|| b"Program not found in the storage".to_vec())?;

            let dispatch_id = queued_dispatch.id();
            let success_reply = queued_dispatch
                .reply()
                .map(|rd| rd.status_code() == 0)
                .unwrap_or_default();
            let gas_limit = GasHandlerOf::<T>::get_limit(dispatch_id)
                .map_err(|_| b"Internal error: unable to get gas limit".to_vec())?;

            let skip_if_allowed = success_reply && gas_limit == 0;

            // todo #1987 : consider to make more common for use in process_queue too
            let build_journal = || {
                let program_id = queued_dispatch.destination();
                let precharged_dispatch = match core_processor::precharge_for_program(
                    &block_config,
                    GasAllowanceOf::<T>::get(),
                    queued_dispatch.into_incoming(gas_limit),
                    actor_id,
                ) {
                    Ok(d) => d,
                    Err(journal) => {
                        return journal;
                    }
                };

                let balance = actor.balance;

                let context = match core_processor::precharge_for_code_length(
                    &block_config,
                    precharged_dispatch,
                    program_id,
                    actor.executable_data,
                ) {
                    Ok(c) => c,
                    Err(journal) => {
                        return journal;
                    }
                };

                let code_id = context.actor_data().code_id;
                let code_len_bytes = match T::CodeStorage::get_code_len(code_id) {
                    None => {
                        unreachable!(
                            "Program '{:?}' exists so do code len '{:?}'",
                            program_id, code_id
                        );
                    }
                    Some(c) => c,
                };

                let context = match core_processor::precharge_for_code(
                    &block_config,
                    context,
                    code_len_bytes,
                ) {
                    Ok(c) => c,
                    Err(journal) => {
                        return journal;
                    }
                };

                let code = match T::CodeStorage::get_code(code_id) {
                    None => {
                        unreachable!(
                            "Program '{:?}' exists so do code '{:?}'",
                            program_id, code_id
                        );
                    }
                    Some(c) => c,
                };

                let schedule = T::Schedule::get();
                let (code, context) = match code.instruction_weights_version()
                    == schedule.instruction_weights.version
                {
                    true => (code, ContextChargedForInstrumentation::from(context)),
                    false => {
                        let context = match core_processor::precharge_for_instrumentation(
                            &block_config,
                            context,
                            code.original_code_len(),
                        ) {
                            Ok(c) => c,
                            Err(journal) => {
                                return journal;
                            }
                        };

                        (Self::reinstrument_code(code_id, &schedule), context)
                    }
                };

                let context = match core_processor::precharge_for_memory(&block_config, context) {
                    Ok(c) => c,
                    Err(journal) => {
                        return journal;
                    }
                };

                let memory_pages = match Self::get_and_track_memory_pages(
                    &mut ext_manager,
                    program_id,
                    &context.actor_data().pages_with_data,
                    lazy_pages_enabled,
                ) {
                    None => unreachable!(),
                    Some(m) => m,
                };

                let (random, bn) = T::Randomness::random(dispatch_id.as_ref());
                let origin = ProgramId::from_origin(source);

                core_processor::process::<ExecutionEnvironment>(
                    &block_config,
                    (context, code, balance, origin).into(),
                    (random.encode(), bn.unique_saturated_into()),
                    memory_pages,
                )
                .unwrap_or_else(|e| unreachable!("core-processor logic invalidated: {}", e))
            };

            let journal = build_journal();

            let get_main_limit = || GasHandlerOf::<T>::get_limit(main_message_id).ok();

            let get_origin_msg_of = |msg_id| {
                GasHandlerOf::<T>::get_origin_key(msg_id)
                    .map_err(|_| b"Internal error: unable to get origin key".to_vec())
            };
            let from_main_chain =
                |msg_id| get_origin_msg_of(msg_id).map(|v| v == main_message_id.into());

            // TODO: Check whether we charge gas fee for submitting code after #646
            for note in journal {
                core_processor::handle_journal(vec![note.clone()], &mut ext_manager);

                if let Some(remaining_gas) = get_main_limit() {
                    min_limit = min_limit.max(initial_gas.saturating_sub(remaining_gas));
                }

                match note {
                    JournalNote::SendDispatch { dispatch, .. } => {
                        let destination =
                            T::AccountId::from_origin(dispatch.destination().into_origin());
                        if MailboxOf::<T>::contains(&destination, &dispatch.id())
                            && from_main_chain(dispatch.id())?
                        {
                            let gas_limit = dispatch
                                .gas_limit()
                                .or_else(|| GasHandlerOf::<T>::get_limit(dispatch.id()).ok())
                                .ok_or_else(|| {
                                    b"Internal error: unable to get gas limit after execution"
                                        .to_vec()
                                })?;

                            reserved = reserved.saturating_add(gas_limit);
                        }
                    }

                    JournalNote::GasBurned { amount, message_id } => {
                        if from_main_chain(message_id)? {
                            burned = burned.saturating_add(amount);
                        }
                    }

                    JournalNote::MessageDispatched {
                        outcome: CoreDispatchOutcome::MessageTrap { trap, program_id },
                        ..
                    } if (program_id == main_program_id || !allow_other_panics)
                        && !(skip_if_allowed && allow_skip_zero_replies) =>
                    {
                        return Err(format!("Program terminated with a trap: {trap}").into_bytes());
                    }

                    _ => (),
                }
            }
        }

        let waited = WaitlistOf::<T>::contains(&main_program_id, &main_message_id);

        Ok(GasInfo {
            min_limit,
            reserved,
            burned,
            may_be_returned: 0,
            waited,
        })
    }

    fn code_with_memory(program_id: ProgramId) -> Result<CodeWithMemoryData, String> {
        let (program, _bn) = ProgramStorageOf::<T>::get_program(program_id)
            .ok_or(String::from("Program not found"))?;

        let program = ActiveProgram::try_from(program)
            .map_err(|e| format!("Get active program error: {e:?}"))?;

        let code_id = CodeId::from_origin(program.code_hash);
        let instrumented_code = T::CodeStorage::get_code(code_id)
            .ok_or_else(|| String::from("Failed to get code for given program id"))?;

        #[cfg(not(feature = "lazy-pages"))]
        let program_pages = Some(
            ProgramStorageOf::<T>::get_program_data_for_pages(
                program_id,
                program.pages_with_data.iter(),
            )
            .map_err(|e| format!("Get program pages data error: {e:?}"))?,
        );

        #[cfg(feature = "lazy-pages")]
        let program_pages = None;

        let allocations = program.allocations;

        Ok(CodeWithMemoryData {
            instrumented_code,
            allocations,
            program_pages,
        })
    }

    pub(crate) fn read_state_using_wasm_impl(
        program_id: ProgramId,
        function: impl Into<String>,
        wasm: Vec<u8>,
        argument: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, String> {
        #[cfg(feature = "lazy-pages")]
        {
            let prefix = ProgramStorageOf::<T>::pages_final_prefix();
            if !lazy_pages::try_to_enable_lazy_pages(prefix) {
                unreachable!("By some reasons we cannot run lazy-pages on this machine");
            }
        }

        let schedule = T::Schedule::get();

        if u32::try_from(wasm.len()).unwrap_or(u32::MAX) > schedule.limits.code_len {
            return Err("Wasm too big".into());
        }

        let code = Code::new_raw_with_rules(
            wasm,
            schedule.instruction_weights.version,
            false,
            |module| schedule.rules(module),
        )
        .map_err(|e| format!("Failed to construct program: {e:?}"))?;

        if u32::try_from(code.code().len()).unwrap_or(u32::MAX) > schedule.limits.code_len {
            return Err("Wasm after instrumentation too big".into());
        }

        let code_and_id = CodeAndId::new(code);
        let code_and_id = InstrumentedCodeAndId::from(code_and_id);

        let instrumented_code = code_and_id.into_parts().0;

        let mut payload = argument.unwrap_or_default();
        payload.append(&mut Self::read_state_impl(program_id)?);

        let block_info = BlockInfo {
            height: Self::block_number().unique_saturated_into(),
            timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
        };

        core_processor::informational::execute_for_reply::<ExecutionEnvironment<String>, String>(
            function.into(),
            instrumented_code,
            None,
            None,
            None,
            payload,
            BlockGasLimitOf::<T>::get() / 4,
            block_info,
        )
    }

    pub(crate) fn read_state_impl(program_id: ProgramId) -> Result<Vec<u8>, String> {
        #[cfg(feature = "lazy-pages")]
        {
            let prefix = ProgramStorageOf::<T>::pages_final_prefix();
            if !lazy_pages::try_to_enable_lazy_pages(prefix) {
                unreachable!("By some reasons we cannot run lazy-pages on this machine");
            }
        }

        log::debug!("Reading state of {program_id:?}");

        let CodeWithMemoryData {
            instrumented_code,
            allocations,
            program_pages,
        } = Self::code_with_memory(program_id)?;

        let block_info = BlockInfo {
            height: Self::block_number().unique_saturated_into(),
            timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
        };

        core_processor::informational::execute_for_reply::<ExecutionEnvironment<String>, String>(
            String::from("state"),
            instrumented_code,
            program_pages,
            Some(allocations),
            Some(program_id),
            Default::default(),
            BlockGasLimitOf::<T>::get() / 4,
            block_info,
        )
    }

    pub(crate) fn read_metahash_impl(program_id: ProgramId) -> Result<H256, String> {
        #[cfg(feature = "lazy-pages")]
        {
            let prefix = ProgramStorageOf::<T>::pages_final_prefix();
            if !lazy_pages::try_to_enable_lazy_pages(prefix) {
                unreachable!("By some reasons we cannot run lazy-pages on this machine");
            }
        }

        log::debug!("Reading metahash of {program_id:?}");

        let CodeWithMemoryData {
            instrumented_code,
            allocations,
            program_pages,
        } = Self::code_with_memory(program_id)?;

        let block_info = BlockInfo {
            height: Self::block_number().unique_saturated_into(),
            timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
        };

        core_processor::informational::execute_for_reply::<ExecutionEnvironment<String>, String>(
            String::from("metahash"),
            instrumented_code,
            program_pages,
            Some(allocations),
            Some(program_id),
            Default::default(),
            BlockGasLimitOf::<T>::get() / 4,
            block_info,
        )
        .and_then(|bytes| {
            H256::decode(&mut bytes.as_ref()).map_err(|_| "Failed to decode hash".into())
        })
    }
}
