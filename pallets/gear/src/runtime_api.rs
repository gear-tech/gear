// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
use crate::queue::QueueStep;
use common::ActiveProgram;
use core::convert::TryFrom;
use frame_support::traits::PalletInfo;
use gear_core::{code::TryNewCodeConfig, pages::WasmPage, program::MemoryInfix};
use gear_wasm_instrument::syscalls::SyscallName;
use sp_runtime::{DispatchErrorWithPostInfo, ModuleError};

// Multiplier 6 was experimentally found as median value for performance,
// security and abilities for calculations on-chain.
pub(crate) const RUNTIME_API_BLOCK_LIMITS_COUNT: u64 = 6;
pub(crate) const ALLOWANCE_LIMIT_ERR: &str = "Calculation gas limit exceeded. Use your own RPC node with `--rpc-calculations-multiplier` parameter raised";

pub(crate) struct CodeWithMemoryData {
    pub instrumented_code: InstrumentedCode,
    pub allocations: BTreeSet<WasmPage>,
    pub memory_infix: MemoryInfix,
}

impl<T: Config> Pallet<T>
where
    T::AccountId: Origin,
{
    fn update_gas_allowance(gas_allowance: u64) {
        GasAllowanceOf::<T>::put(gas_allowance);
        QueueProcessingOf::<T>::allow();
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn calculate_gas_info_impl(
        origin: H256,
        kind: HandleKind,
        initial_gas: u64,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        allow_skip_zero_replies: bool,
        allowance_multiplier: Option<u64>,
    ) -> Result<GasInfo, Vec<u8>> {
        Self::enable_lazy_pages();

        let origin = origin.cast();
        let value = value.unique_saturated_into();

        let origin_balance = CurrencyOf::<T>::free_balance(&origin);

        let value_for_gas =
            <T as pallet_gear_bank::Config>::GasMultiplier::get().gas_to_value(initial_gas);

        let required_balance = CurrencyOf::<T>::minimum_balance()
            .saturating_add(value_for_gas)
            .saturating_add(value);

        let _ = CurrencyOf::<T>::deposit_creating(
            &origin,
            required_balance.saturating_sub(origin_balance),
        );

        let who = frame_support::dispatch::RawOrigin::Signed(origin);

        QueueOf::<T>::clear();

        let map_extrinsic_err = |extrinsic_name: &'static str,
                                 e: DispatchErrorWithPostInfo<PostDispatchInfo>|
         -> Vec<u8> {
            let error_module_idx = if let DispatchError::Module(ModuleError { index, .. }) = e.error
            {
                Some(index as usize)
            } else {
                None
            };

            let error_message: &'static str = e.into();

            let gear_module_idx =
                <<T as frame_system::Config>::PalletInfo as PalletInfo>::index::<Pallet<T>>()
                    .expect("No index found for the gear pallet in the runtime!");

            let mut res = format!("Extrinsic `gear.{extrinsic_name}` failed: '{error_message}'");

            if let Some(module_idx) = error_module_idx.filter(|i| *i != gear_module_idx) {
                res = format!("{res} (pallet index of the error: {module_idx}");
            }

            res.into_bytes()
        };

        let internal_err = |message: &'static str| -> Vec<u8> {
            format!("Internal error: entered unreachable code '{message}'").into_bytes()
        };

        match kind {
            HandleKind::Init(code) => {
                let salt = b"calculate_gas_salt".to_vec();

                Self::upload_program(who.into(), code, salt, payload, initial_gas, value, false)
                    .map_err(|e| map_extrinsic_err("upload_program", e))?;
            }
            HandleKind::InitByHash(code_id) => {
                let salt = b"calculate_gas_salt".to_vec();

                Self::create_program(
                    who.into(),
                    code_id,
                    salt,
                    payload,
                    initial_gas,
                    value,
                    false,
                )
                .map_err(|e| map_extrinsic_err("create_program", e))?;
            }
            HandleKind::Handle(destination) => {
                Self::send_message(who.into(), destination, payload, initial_gas, value, false)
                    .map_err(|e| map_extrinsic_err("send_message", e))?;
            }
            HandleKind::Reply(reply_to_id, _status_code) => {
                Self::send_reply(who.into(), reply_to_id, payload, initial_gas, value, false)
                    .map_err(|e| map_extrinsic_err("send_reply", e))?;
            }
            HandleKind::Signal(_signal_from, _status_code) => {
                return Err(b"Gas calculation for `handle_signal` is not supported".to_vec());
            }
        };

        let (main_message_id, main_program_id) = QueueOf::<T>::iter()
            .next()
            .ok_or_else(|| internal_err("Failed to get last message from the queue"))
            .and_then(|queued| {
                queued
                    .map_err(|_| internal_err("Failed to extract queued dispatch"))
                    .map(|dispatch| (dispatch.id(), dispatch.destination()))
            })?;

        let mut block_config = Self::block_config();
        block_config.forbidden_funcs = [SyscallName::GasAvailable].into();

        let mut min_limit = 0;
        let mut reserved = 0;
        let mut burned = 0;

        let mut ext_manager = ExtManager::<T>::default();

        let gas_allowance = allowance_multiplier
            .unwrap_or(RUNTIME_API_BLOCK_LIMITS_COUNT)
            .saturating_mul(BlockGasLimitOf::<T>::get());

        Self::update_gas_allowance(gas_allowance);

        // Create an instance of a builtin dispatcher.
        let builtin_dispatcher = T::BuiltinProvider::provide();

        loop {
            if QueueProcessingOf::<T>::denied() {
                return Err(ALLOWANCE_LIMIT_ERR.as_bytes().to_vec());
            }

            let Some(queued_dispatch) =
                QueueOf::<T>::dequeue().map_err(|_| internal_err("Message queue corrupted"))?
            else {
                break;
            };

            let actor_id = queued_dispatch.destination();
            let dispatch_id = queued_dispatch.id();

            let gas_limit = GasHandlerOf::<T>::get_limit(dispatch_id)
                .map_err(|_| internal_err("Failed to get gas limit"))?;

            let (journal, skip_if_allowed) =
                if let Some(builtin_id) = builtin_dispatcher.lookup(&actor_id) {
                    (
                        builtin_dispatcher.dispatch(builtin_id, queued_dispatch, gas_limit),
                        false,
                    )
                } else {
                    let balance = CurrencyOf::<T>::free_balance(&actor_id.cast());

                    let success_reply = queued_dispatch
                        .reply_details()
                        .map(|rd| rd.to_reply_code().is_success())
                        .unwrap_or(false);

                    let step = QueueStep {
                        block_config: &block_config,
                        ext_manager: &mut ext_manager,
                        gas_limit,
                        dispatch: queued_dispatch,
                        balance: balance.unique_saturated_into(),
                    };

                    (Self::run_queue_step(step), success_reply && gas_limit == 0)
                };

            let get_main_limit = || {
                // For case when node is not consumed and has any (even zero) balance
                // it means that it burned/sent all the funds and we must return it.
                //
                // For case when node is consumed and has zero balance it means that
                // node moved its funds upstream to its ancestor. So this shouldn't
                // be returned.
                //
                // For case when node is consumed and has non zero balance it means
                // that it has gasless child that will consume gas further. So we
                // handle this value as well.
                GasHandlerOf::<T>::get_limit(main_message_id)
                    .ok()
                    .or_else(|| {
                        GasHandlerOf::<T>::get_limit_consumed(main_message_id)
                            .ok()
                            .filter(|limit| !limit.is_zero())
                    })
            };

            let get_origin_msg_of = |msg_id| {
                GasHandlerOf::<T>::get_origin_key(msg_id)
                    .map_err(|_| internal_err("Failed to get origin key"))
            };
            let from_main_chain =
                |msg_id| get_origin_msg_of(msg_id).map(|v| v == main_message_id.into());

            // TODO: Check whether we charge gas fee for submitting code after #646
            for note in journal {
                core_processor::handle_journal(vec![note.clone()], &mut ext_manager);

                match get_main_limit() {
                    Some(remaining_gas) => {
                        min_limit = min_limit.max(initial_gas.saturating_sub(remaining_gas))
                    }
                    None => match note {
                        // take into account that 'wait' syscall greedily consumes all available gas.
                        // 'wait_for' and 'wait_up_to' should not consume all available gas
                        // because of the limited durations. If a duration is a big enough then it
                        // won't matter how to calculate the limit: it will be the same.
                        JournalNote::WaitDispatch { ref dispatch, .. }
                            if from_main_chain(dispatch.id())? =>
                        {
                            min_limit = initial_gas
                        }
                        _ => (),
                    },
                }

                match note {
                    JournalNote::SendDispatch { dispatch, .. } => {
                        let destination = dispatch.destination().cast();

                        if MailboxOf::<T>::contains(&destination, &dispatch.id())
                            && from_main_chain(dispatch.id())?
                        {
                            let gas_limit = dispatch
                                .gas_limit()
                                .or_else(|| GasHandlerOf::<T>::get_limit(dispatch.id()).ok())
                                .ok_or_else(|| {
                                    internal_err("Failed to get gas limit after execution")
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
                        outcome:
                            CoreDispatchOutcome::MessageTrap { trap, .. }
                            | CoreDispatchOutcome::InitFailure { reason: trap, .. },
                        message_id,
                        ..
                    } if (message_id == main_message_id || !allow_other_panics)
                        && !(skip_if_allowed && allow_skip_zero_replies) =>
                    {
                        return Err(
                            format!("Program terminated with a trap: '{trap}'").into_bytes()
                        );
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
        let program = ProgramStorageOf::<T>::get_program(program_id)
            .ok_or(String::from("Program not found"))?;

        let program = ActiveProgram::try_from(program)
            .map_err(|e| format!("Get active program error: {e:?}"))?;

        let instrumented_code = T::CodeStorage::get_code(program.code_hash.cast())
            .ok_or_else(|| String::from("Failed to get code for given program id"))?;

        Ok(CodeWithMemoryData {
            instrumented_code,
            allocations: program.allocations,
            memory_infix: program.memory_infix,
        })
    }

    pub(crate) fn read_state_using_wasm_impl(
        program_id: ProgramId,
        payload: Vec<u8>,
        function: impl Into<String>,
        wasm: Vec<u8>,
        argument: Option<Vec<u8>>,
        allowance_multiplier: Option<u64>,
    ) -> Result<Vec<u8>, String> {
        Self::enable_lazy_pages();

        let schedule = T::Schedule::get();

        if u32::try_from(wasm.len()).unwrap_or(u32::MAX) > schedule.limits.code_len {
            return Err("Wasm too big".into());
        }

        let code = Code::try_new_mock_with_rules(
            wasm,
            |module| schedule.rules(module),
            TryNewCodeConfig::new_no_exports_check(),
        )
        .map_err(|e| format!("Failed to construct program: {e:?}"))?;

        if u32::try_from(code.code().len()).unwrap_or(u32::MAX) > schedule.limits.code_len {
            return Err("Wasm after instrumentation too big".into());
        }

        let code_and_id = CodeAndId::new(code);
        let code_and_id = InstrumentedCodeAndId::from(code_and_id);

        let instrumented_code = code_and_id.into_parts().0;

        let payload_arg = payload;
        let mut payload = argument.unwrap_or_default();
        payload.append(&mut Self::read_state_impl(
            program_id,
            payload_arg,
            allowance_multiplier,
        )?);

        let block_info = BlockInfo {
            height: Self::block_number().unique_saturated_into(),
            timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
        };

        let gas_allowance = allowance_multiplier
            .unwrap_or(RUNTIME_API_BLOCK_LIMITS_COUNT)
            .saturating_mul(BlockGasLimitOf::<T>::get());

        Self::update_gas_allowance(gas_allowance);

        core_processor::informational::execute_for_reply::<Ext, String>(
            function.into(),
            instrumented_code,
            None,
            None,
            payload,
            gas_allowance,
            block_info,
        )
    }

    pub(crate) fn read_state_impl(
        program_id: ProgramId,
        payload: Vec<u8>,
        allowance_multiplier: Option<u64>,
    ) -> Result<Vec<u8>, String> {
        Self::enable_lazy_pages();

        log::debug!("Reading state of {program_id:?}");

        let CodeWithMemoryData {
            instrumented_code,
            allocations,
            memory_infix,
        } = Self::code_with_memory(program_id)?;

        let block_info = BlockInfo {
            height: Self::block_number().unique_saturated_into(),
            timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
        };

        let gas_allowance = allowance_multiplier
            .unwrap_or(RUNTIME_API_BLOCK_LIMITS_COUNT)
            .saturating_mul(BlockGasLimitOf::<T>::get());

        Self::update_gas_allowance(gas_allowance);

        core_processor::informational::execute_for_reply::<Ext, String>(
            String::from("state"),
            instrumented_code,
            Some(allocations),
            Some((program_id, memory_infix)),
            payload,
            gas_allowance,
            block_info,
        )
    }

    pub(crate) fn read_metahash_impl(
        program_id: ProgramId,
        allowance_multiplier: Option<u64>,
    ) -> Result<H256, String> {
        Self::enable_lazy_pages();

        log::debug!("Reading metahash of {program_id:?}");

        let CodeWithMemoryData {
            instrumented_code,
            allocations,
            memory_infix,
        } = Self::code_with_memory(program_id)?;

        let block_info = BlockInfo {
            height: Self::block_number().unique_saturated_into(),
            timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
        };

        let gas_allowance = allowance_multiplier
            .unwrap_or(RUNTIME_API_BLOCK_LIMITS_COUNT)
            .saturating_mul(BlockGasLimitOf::<T>::get());

        Self::update_gas_allowance(gas_allowance);

        core_processor::informational::execute_for_reply::<Ext, String>(
            String::from("metahash"),
            instrumented_code,
            Some(allocations),
            Some((program_id, memory_infix)),
            Default::default(),
            gas_allowance,
            block_info,
        )
        .and_then(|bytes| {
            H256::decode(&mut bytes.as_ref()).map_err(|_| "Failed to decode hash".into())
        })
    }
}
