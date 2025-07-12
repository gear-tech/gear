// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
use core::convert::TryFrom;
use frame_support::{dispatch::RawOrigin, traits::PalletInfo};
use gear_core::{
    code::{InstrumentedCodeAndMetadata, TryNewCodeConfig},
    pages::{numerated::tree::IntervalsTree, WasmPage},
    program::{ActiveProgram, MemoryInfix},
    rpc::ReplyInfo,
};
use gear_wasm_instrument::syscalls::SyscallName;
use sp_runtime::{DispatchErrorWithPostInfo, ModuleError};

// Multiplier 6 was experimentally found as median value for performance,
// security and abilities for calculations on-chain.
pub(crate) const RUNTIME_API_BLOCK_LIMITS_COUNT: u64 = 6;
pub(crate) const ALLOWANCE_LIMIT_ERR: &str = "Calculation gas limit exceeded. Use your own RPC node with `--rpc-calculations-multiplier` parameter raised";

pub(crate) struct CodeWithMemoryData {
    pub instrumented_code: InstrumentedCode,
    pub code_metadata: CodeMetadata,
    pub allocations: IntervalsTree<WasmPage>,
    pub memory_infix: MemoryInfix,
}

impl<T: Config> Pallet<T>
where
    T::AccountId: Origin,
{
    // Internal implementation of RPC call `gear_calculate_replyForHandle(..)`.
    //
    // The RPC call is used to figure out the reply that would be send
    // on calling `Gear::send_message(..)` with following arguments.
    pub(crate) fn calculate_reply_for_handle_impl(
        origin: H256,
        destination: ActorId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
        allowance_multiplier: u64,
    ) -> Result<ReplyInfo, String> {
        // Enabling lazy-pages for this thread.
        Self::enable_lazy_pages();

        // Clearing queue.
        QueueOf::<T>::clear();

        // Calculating gas allowance for a whole operation,
        // according to allowance multiplier.
        let gas_allowance = allowance_multiplier.saturating_mul(BlockGasLimitOf::<T>::get());

        // Updating gas allowance with calculated value.
        Self::update_gas_allowance(gas_allowance);

        // Casting types into runtime assoc-s.
        let origin = origin.cast();
        let value = value.unique_saturated_into();

        // Preparing origin balance for extrinsic expenses.
        let who = Self::prepare_origin_account(origin, gas_limit, value);

        // Executing `send_message` call.
        Self::send_message(who.into(), destination, payload, gas_limit, value, false)
            .map_err(|e| Self::dispatch_err_to_string("send_message", e))?;

        // Looking up queue head for message id sent above.
        let (message_id, _) = Self::queue_head()?;

        // Creating builtin dispatcher for queue processing.
        let (builtin_dispatcher, _) = T::BuiltinDispatcherFactory::create();

        // Creating new manager for queue processing.
        let mut ext_manager = ExtManager::<T>::new(builtin_dispatcher);

        // Queue processing loop.
        //
        // Running queue head message if exists.
        while let Some((_, journal, _)) = Self::dequeue_head_and_run(&mut ext_manager, None)? {
            // Looking through all notes in order to find required reply.
            for note in &journal {
                // Only paying attention on dispatch sends.
                let JournalNote::SendDispatch { dispatch, .. } = note else {
                    continue;
                };

                // Only paying attention if replies to `message_id`.
                if let Some(code) = dispatch
                    .reply_details()
                    .map(ReplyDetails::into_parts)
                    .and_then(|(replied_to, code)| replied_to.eq(&message_id).then_some(code))
                {
                    return Ok(ReplyInfo {
                        payload: dispatch.payload_bytes().to_vec(),
                        value: dispatch.value(),
                        code,
                    });
                }
            }

            // Processing notes since reply wasn't found.
            core_processor::handle_journal(journal, &mut ext_manager);

            // If some message overcame block allowance, aborting processing.
            if QueueProcessingOf::<T>::denied() {
                return Err(ALLOWANCE_LIMIT_ERR.to_string());
            }
        }

        // Ran out of messages in queue.
        Err(String::from("Queue is empty, but reply wasn't found"))
    }

    // Internal implementation of RPC calls `gear_calculate*Entry*Gas(..)`.
    //
    // The RPC call is used to figure out required gas amount for successful
    // execution of the message.
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
    ) -> Result<GasInfo, String> {
        // Enabling lazy-pages for this thread.
        Self::enable_lazy_pages();

        // Clearing queue.
        QueueOf::<T>::clear();

        // Calculating gas allowance for a whole operation,
        // according to allowance multiplier.
        let gas_allowance = allowance_multiplier
            .unwrap_or(RUNTIME_API_BLOCK_LIMITS_COUNT)
            .saturating_mul(BlockGasLimitOf::<T>::get());

        // Updating gas allowance with calculated value.
        Self::update_gas_allowance(gas_allowance);

        // Casting types into runtime assoc-s.
        let origin = origin.cast();
        let value = value.unique_saturated_into();

        // Preparing origin balance for extrinsic expenses.
        let who = Self::prepare_origin_account(origin, initial_gas, value);

        match kind {
            // Executing `upload_program` call.
            HandleKind::Init(code) => {
                let salt = b"calculate_gas_salt".to_vec();

                Self::upload_program(who.into(), code, salt, payload, initial_gas, value, false)
                    .map_err(|e| Self::dispatch_err_to_string("upload_program", e))?;
            }

            // Executing `create_program` call.
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
                .map_err(|e| Self::dispatch_err_to_string("create_program", e))?;
            }

            // Executing `send_message` call.
            HandleKind::Handle(destination) => {
                Self::send_message(who.into(), destination, payload, initial_gas, value, false)
                    .map_err(|e| Self::dispatch_err_to_string("send_message", e))?;
            }

            // Executing `send_reply` call.
            HandleKind::Reply(reply_to_id, _status_code) => {
                Self::send_reply(who.into(), reply_to_id, payload, initial_gas, value, false)
                    .map_err(|e| Self::dispatch_err_to_string("send_reply", e))?;
            }

            // Handle signal forbidden call.
            HandleKind::Signal(_signal_from, _status_code) => {
                return Err(String::from(
                    "Gas calculation for `handle_signal` is not supported",
                ));
            }
        };

        // Looking up queue head for message id and destination sent above.
        let (main_message_id, main_program_id) = Self::queue_head()?;

        // Creating builtin dispatcher for queue processing.
        let (builtin_dispatcher, _) = T::BuiltinDispatcherFactory::create();

        // Creating new manager for queue processing.
        let mut ext_manager = ExtManager::<T>::new(builtin_dispatcher);

        // Creating forbidden funcs registry.
        let forbidden_funcs = [SyscallName::GasAvailable];

        // Getter for gas limit of the root message.
        //
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
        let get_main_limit = || {
            GasHandlerOf::<T>::get_limit(main_message_id)
                .ok()
                .or_else(|| {
                    GasHandlerOf::<T>::get_limit_consumed(main_message_id)
                        .ok()
                        .filter(|limit| !limit.is_zero())
                })
        };

        // Getter for identifying if message was in primary messages chain.
        let from_main_chain = |msg_id| {
            GasHandlerOf::<T>::get_origin_key(msg_id)
                .map(|v| v == main_message_id.into())
                .map_err(|_| Self::internal_err_string("Failed to get origin key"))
        };

        // Result to be returned.
        let mut gas_info: GasInfo = Default::default();

        // Queue processing loop.
        //
        // Running queue head message if exists.
        while let Some((processed, journal, by_builtin)) =
            Self::dequeue_head_and_run(&mut ext_manager, Some(forbidden_funcs.into()))?
        {
            // Defining if success reply was processed.
            let success_reply = processed
                .reply_details()
                .map(|rd| rd.to_reply_code().is_success())
                .unwrap_or(false);

            // Extracting infallibly gas limit of processed message.
            let gas_limit = processed.gas_limit().expect("Infallible");

            // Defining if we skip checks for this message if allowed to.
            let skip_if_allowed = !by_builtin && success_reply && gas_limit == 0;

            // Looking through all notes in order to calculate gas properly.
            for note in journal {
                // Processing note.
                core_processor::handle_journal(vec![note.clone()], &mut ext_manager);

                // If some message overcame block allowance, aborting processing.
                if QueueProcessingOf::<T>::denied() {
                    return Err(ALLOWANCE_LIMIT_ERR.to_string());
                }

                // Querying gas limit of the main messages chain.
                match get_main_limit() {
                    // If some limit still exist, than checking the highest
                    // diff from initial gas as calculated value.
                    Some(remaining_gas) => {
                        gas_info.min_limit = gas_info
                            .min_limit
                            .max(initial_gas.saturating_sub(remaining_gas));
                    }

                    // If limit no longer exists we need to check others for
                    // infinite wait if they belong to main messages chain.
                    None => {
                        // Take into account that 'wait' syscall greedily
                        // consumes all available gas.
                        // Meanwhile, 'wait_for' and 'wait_up_to' should not
                        // consume all available gas because of the limited
                        // durations. If a duration is a big enough then it
                        // won't matter how to calculate the limit:
                        // it will be the same.
                        if let JournalNote::WaitDispatch {
                            waited_type: MessageWaitedType::Wait,
                            ref dispatch,
                            ..
                        } = note
                            && from_main_chain(dispatch.id())?
                        {
                            gas_info.min_limit = initial_gas;
                        }
                    }
                }

                // Parsing other types of the node for extra actions.
                match note {
                    // Checking sending for mailbox insertion (e.g. reserve).
                    JournalNote::SendDispatch { dispatch, .. } => {
                        // Extracting and casting destination to AccountId.
                        let destination = dispatch.destination().cast();

                        // Checking mailbox insertion and if newly created
                        // dispatch is from main chain.
                        //
                        // NOTE: to pass `from_main_chain` call, message should
                        // exist in system: at least in `Mailbox`.
                        if MailboxOf::<T>::contains(&destination, &dispatch.id())
                            && from_main_chain(dispatch.id())?
                        {
                            // Querying reserved balance for mailbox storing.
                            //
                            // NOTE: here goes extraction of the gas directly
                            // from message, if gasless sent, than it's
                            // queried from the storage tree.
                            let gas_limit = dispatch
                                .gas_limit()
                                .or_else(|| GasHandlerOf::<T>::get_limit(dispatch.id()).ok())
                                .ok_or_else(|| {
                                    Self::internal_err_string(
                                        "Failed to get gas limit after execution",
                                    )
                                })?;

                            gas_info.reserved = gas_info.reserved.saturating_add(gas_limit);
                        }
                    }

                    // Burning gas from main messages chain.
                    JournalNote::GasBurned { amount, message_id } => {
                        if from_main_chain(message_id)? {
                            gas_info.burned = gas_info.burned.saturating_add(amount);
                        }
                    }

                    // Checking execution for panic happened.
                    JournalNote::MessageDispatched {
                        outcome:
                            CoreDispatchOutcome::MessageTrap { trap, .. }
                            | CoreDispatchOutcome::InitFailure { reason: trap, .. },
                        message_id,
                        ..
                    } if (message_id == main_message_id || !allow_other_panics)
                        && !(skip_if_allowed && allow_skip_zero_replies) =>
                    {
                        return Err(format!("Program terminated with a trap: '{trap}'"));
                    }

                    _ => (),
                }
            }
        }

        // Defining if message is kept by waitlist.
        gas_info.waited = WaitlistOf::<T>::contains(&main_program_id, &main_message_id);

        // Returning result.
        Ok(gas_info)
    }

    pub(crate) fn read_state_using_wasm_impl(
        program_id: ActorId,
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

        if u32::try_from(code.instrumented_code().bytes().len()).unwrap_or(u32::MAX)
            > schedule.limits.code_len
        {
            return Err("Wasm after instrumentation too big".into());
        }

        let code_and_id = CodeAndId::new(code);

        let (_, instrumented_code, code_metadata) = code_and_id.into_parts().0.into_parts();

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
            code_metadata,
            None,
            None,
            payload,
            gas_allowance,
            block_info,
        )
    }

    pub(crate) fn read_state_impl(
        program_id: ActorId,
        payload: Vec<u8>,
        allowance_multiplier: Option<u64>,
    ) -> Result<Vec<u8>, String> {
        Self::enable_lazy_pages();

        log::debug!("Reading state of {program_id:?}");

        let CodeWithMemoryData {
            instrumented_code,
            code_metadata,
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
            code_metadata,
            Some(allocations),
            Some((program_id, memory_infix)),
            payload,
            gas_allowance,
            block_info,
        )
    }

    pub(crate) fn read_metahash_impl(
        program_id: ActorId,
        allowance_multiplier: Option<u64>,
    ) -> Result<H256, String> {
        Self::enable_lazy_pages();

        log::debug!("Reading metahash of {program_id:?}");

        let CodeWithMemoryData {
            instrumented_code,
            code_metadata,
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
            code_metadata,
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

    // Returns code and allocations of the given program id.
    fn code_with_memory(program_id: ActorId) -> Result<CodeWithMemoryData, String> {
        // Load active program from storage.
        let program: ActiveProgram<_> = ProgramStorageOf::<T>::get_program(program_id)
            .ok_or(String::from("Program not found"))?
            .try_into()
            .map_err(|e| format!("Get active program error: {e:?}"))?;

        let code_id = program.code_id.cast();

        let code_metadata = T::CodeStorage::get_code_metadata(code_id)
            .ok_or_else(|| format!("Code '{code_id:?}' not found for program '{program_id:?}'"))?;

        let schedule = T::Schedule::get();

        // Check if the code needs to be reinstrumented.
        let needs_reinstrumentation = match code_metadata.instrumentation_status() {
            InstrumentationStatus::NotInstrumented => {
                log::debug!(
                    "Instrumented code doesn't exists for program '{program_id:?}' \
                     we need to instrument it with instructions weights version {}",
                    schedule.instruction_weights.version
                );

                true
            }
            InstrumentationStatus::Instrumented { version, .. } => {
                version != schedule.instruction_weights.version
            }
            InstrumentationStatus::InstrumentationFailed { version } => {
                if version == schedule.instruction_weights.version {
                    return Err(format!(
                        "Re-instrumentation already failed for program '{program_id:?}' \
                        with instructions weights version {version}"
                    ));
                }

                true
            }
        };

        let instrumented_code_and_metadata = if needs_reinstrumentation {
            Pallet::<T>::reinstrument_code(code_id, code_metadata, &schedule)
                .map_err(|e| format!("Code {code_id:?} failed reinstrumentation: {e:?}"))?
        } else {
            let instrumented_code =
                T::CodeStorage::get_instrumented_code(code_id).ok_or_else(|| {
                    format!("Program '{program_id:?}' exists so must do code '{code_id:?}'")
                })?;

            InstrumentedCodeAndMetadata {
                instrumented_code,
                metadata: code_metadata,
            }
        };

        let allocations = ProgramStorageOf::<T>::allocations(program_id).unwrap_or_default();

        Ok(CodeWithMemoryData {
            instrumented_code: instrumented_code_and_metadata.instrumented_code,
            code_metadata: instrumented_code_and_metadata.metadata,
            allocations,
            memory_infix: program.memory_infix,
        })
    }

    // Prepares account id to be able to execute some extrinsic in terms of funds.
    fn prepare_origin_account(
        origin: AccountIdOf<T>,
        gas: u64,
        value: BalanceOf<T>,
    ) -> RawOrigin<AccountIdOf<T>> {
        // Querying transferrable balance of the origin taking into account a possibility of
        // a part of its `free` balance being `frozen`.
        let origin_balance = <CurrencyOf<T> as fungible::Inspect<_>>::reducible_balance(
            &origin,
            Preservation::Expendable,
            Fortitude::Polite,
        );

        // Calculating amount of value to be paid for gas.
        let value_for_gas = <T as pallet_gear_bank::Config>::GasMultiplier::get().gas_to_value(gas);

        // Required balance of the account.
        let required_balance = CurrencyOf::<T>::minimum_balance()
            .saturating_add(value_for_gas)
            .saturating_add(value);

        // Updating balance of the account.
        let _ = CurrencyOf::<T>::deposit_creating(
            &origin,
            required_balance.saturating_sub(origin_balance),
        );

        // Returning origin account as signed origin.
        RawOrigin::Signed(origin)
    }

    // Returns none if queue is empty, otherwise - processed message,
    // resulting journal notes of the processing and bool, defining
    // was it processed by builtin actor or not.
    fn dequeue_head_and_run(
        ext_manager: &mut ExtManager<T>,
        forbidden_funcs: Option<BTreeSet<SyscallName>>,
    ) -> Result<Option<(Dispatch, Vec<JournalNote>, bool)>, String> {
        // Extracting queued dispatch.
        let head =
            QueueOf::<T>::dequeue().map_err(|_| Self::internal_err_string("Queue corrupted"))?;

        let Some(dispatch) = head else {
            return Ok(None);
        };

        // Extracting destination from dispatch.
        let destination = dispatch.destination();

        // Querying gas limit for dispatch.
        let gas_limit = GasHandlerOf::<T>::get_limit(dispatch.id())
            .map_err(|_| Self::internal_err_string("Failed to get gas limit"))?;

        // Storing processed dispatch.
        let processed = Dispatch::new(
            dispatch.kind(),
            Message::new(
                dispatch.id(),
                dispatch.source(),
                dispatch.destination(),
                dispatch
                    .payload_bytes()
                    .to_vec()
                    .try_into()
                    .expect("Infallible"),
                Some(gas_limit),
                dispatch.value(),
                dispatch.details(),
            ),
        );

        // Processing of the message, if destination is builtin actor.
        let builtin_dispatcher = ext_manager.builtins();
        if let Some(info) = builtin_dispatcher.lookup(&destination) {
            let journal = builtin_dispatcher.run(info, dispatch, gas_limit);
            return Ok(Some((processed, journal, true)));
        }

        let mut block_config = Self::block_config();

        if let Some(forbidden_funcs) = forbidden_funcs {
            block_config.forbidden_funcs = forbidden_funcs;
        }

        // Program's balance that it can spend during a message execution.
        let disposable_balance = <CurrencyOf<T> as fungible::Inspect<_>>::reducible_balance(
            &destination.cast(),
            Preservation::Expendable,
            Fortitude::Polite,
        );

        // Processing of the message, if destination is common program.
        let journal = Self::run_queue_step(QueueStep {
            block_config: &block_config,
            gas_limit,
            dispatch,
            balance: disposable_balance.unique_saturated_into(),
        });

        Ok(Some((processed, journal, false)))
    }

    // Converts given dispatch error into dedicated runtime api string format.
    fn dispatch_err_to_string(
        extrinsic_name: &'static str,
        e: DispatchErrorWithPostInfo<PostDispatchInfo>,
    ) -> String {
        // Extracting index of module returned error, if possible.
        let error_module_idx = match e.error {
            DispatchError::Module(ModuleError { index, .. }) => Some(index as usize),
            _ => None,
        };

        // Converting dispatch error into string representation in default impl.
        let error_message: &'static str = e.into();

        // Creating result message.
        let mut res = format!("Extrinsic `gear.{extrinsic_name}` failed: '{error_message}'");

        // Extracting `pallet_gear` index from runtime to compare with dispatch error.
        let Some(gear_module_idx) = PalletInfoOf::<T>::index::<Self>() else {
            return Self::internal_err_string("No index found for `pallet_gear` in the runtime");
        };

        // Appending result message with pallet index returned error, if not this.
        if let Some(module_idx) = error_module_idx.filter(|i| *i != gear_module_idx) {
            res = format!("{res} (pallet index of the error: {module_idx}");
        }

        res
    }

    // Queries first element of the queue and extracts its message id and destination.
    fn queue_head() -> Result<(MessageId, ActorId), String> {
        QueueOf::<T>::iter()
            .next()
            .ok_or_else(|| Self::internal_err_string("Failed to get last message from the queue"))
            .and_then(|queued| {
                queued
                    .map(|dispatch| (dispatch.id(), dispatch.destination()))
                    .map_err(|_| Self::internal_err_string("Failed to extract queued dispatch"))
            })
    }

    // Updates gas allowance and allows queue processing.
    fn update_gas_allowance(gas_allowance: u64) {
        GasAllowanceOf::<T>::put(gas_allowance);
        QueueProcessingOf::<T>::allow();
    }

    // Formats given message into dedicated runtime api string format.
    fn internal_err_string(message: impl ToString) -> String {
        format!(
            "Internal error: entered unreachable code '{}'",
            message.to_string()
        )
    }
}
