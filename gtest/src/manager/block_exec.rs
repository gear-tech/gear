// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use super::*;
use crate::{
    WasmProgram,
    builtins::{self, BLS12_381_ID, BlsOpsGasCostsImpl, ETH_BRIDGE_ID},
    state::{
        blocks,
        programs::{GTestProgram, PLACEHOLDER_MESSAGE_ID},
    },
};
use builtins_common::{
    BuiltinActorError, BuiltinContext,
    bls12_381::{self, Bls12_381OpsLowLevel},
};
use core_processor::{
    ContextCharged, ForProgram, ProcessExecutionContext, SystemReservationContext,
};
use gear_core::{
    code::{InstrumentedCodeAndMetadata, MAX_WASM_PAGES_AMOUNT},
    gas::GasCounter,
    message::{ContextOutcomeDrain, DispatchKind, MessageContext, ReplyPacket, StoredDispatch},
    program::ProgramState,
    str::LimitedStr,
};
use parity_scale_codec::Encode;

impl ExtManager {
    pub(crate) fn validate_and_route_dispatch(&mut self, dispatch: Dispatch) -> MessageId {
        self.validate_dispatch(&dispatch);
        let gas_limit = dispatch
            .gas_limit()
            .unwrap_or_else(|| unreachable!("message from program API always has gas"));
        self.gas_tree
            .create(
                dispatch.source(),
                dispatch.id(),
                gas_limit,
                dispatch.is_reply(),
            )
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
        self.route_dispatch(dispatch)
    }

    fn validate_dispatch(&mut self, dispatch: &Dispatch) {
        let source = dispatch.source();
        let destination = dispatch.destination();

        if ProgramsStorageManager::is_program(source) {
            usage_panic!(
                "Sending messages allowed only from users id. Please, provide user id as source."
            );
        }

        assert!(
            self.no_code_program.is_empty(),
            "internal error: no code programs set is not empty"
        );

        // User must exist
        if !Accounts::exists(source) {
            usage_panic!("User's {source} balance is zero; mint value to it first.");
        }

        if !ProgramsStorageManager::is_active_program(destination) {
            usage_panic!("User message can't be sent to non active program");
        }

        let is_init_msg = dispatch.kind().is_init();
        // We charge ED only for init messages
        let maybe_ed = if is_init_msg { EXISTENTIAL_DEPOSIT } else { 0 };
        let balance = Accounts::balance(source);

        let gas_limit = dispatch
            .gas_limit()
            .unwrap_or_else(|| unreachable!("message from program API always has gas"));

        if gas_limit > MAX_USER_GAS_LIMIT {
            usage_panic!(
                "User message gas limit ({gas_limit}) is greater than \
                maximum allowed ({MAX_USER_GAS_LIMIT})."
            );
        };

        let gas_value = GAS_MULTIPLIER.gas_to_value(gas_limit);

        // Check sender has enough balance to cover dispatch costs
        if balance < { dispatch.value() + gas_value + maybe_ed } {
            usage_panic!(
                "Insufficient balance: user ({}) tries to send \
                ({}) value, ({}) gas and ED ({}), while his balance ({:?}). \
                Please, mint more balance to the user.",
                source,
                dispatch.value(),
                gas_value,
                maybe_ed,
                balance,
            );
        }

        // Charge for program ED upon creation
        if is_init_msg {
            Accounts::transfer(source, destination, EXISTENTIAL_DEPOSIT, false);
        }

        if dispatch.is_error_reply() {
            panic!("Internal error: users are not allowed to send error replies");
        }

        // It's necessary to deposit value so the source would have enough
        // balance locked (in gear-bank) for future value processing.
        if dispatch.value() != 0 {
            self.bank.deposit_value(source, dispatch.value(), false);
        }

        // Deposit gas
        self.bank.deposit_gas(source, gas_limit, false);
    }

    pub(crate) fn route_dispatch(&mut self, dispatch: Dispatch) -> MessageId {
        let stored_dispatch = dispatch.into_stored();
        if ProgramsStorageManager::is_user(stored_dispatch.destination()) {
            panic!("Program API only sends message to programs.")
        }

        let message_id = stored_dispatch.id();
        self.dispatches.push_back(stored_dispatch);

        message_id
    }

    pub(crate) fn run_new_block(&mut self, allowance: Gas) -> BlockRunResult {
        self.gas_allowance = allowance;
        self.blocks_manager.next_block();
        let new_block_bn = self.block_height();

        log::debug!("⚙️  Initialization of block #{new_block_bn}");

        self.process_tasks(new_block_bn);
        let total_processed = self.process_messages();

        log::debug!("⚙️  Finalization of block #{new_block_bn}");

        // Clean up no code programs for the next block
        self.no_code_program.clear();

        BlockRunResult {
            block_info: self.blocks_manager.get(),
            gas_allowance_spent: GAS_ALLOWANCE - self.gas_allowance,
            succeed: mem::take(&mut self.succeed),
            failed: mem::take(&mut self.failed),
            not_executed: mem::take(&mut self.not_executed),
            total_processed,
            log: mem::take(&mut self.log)
                .into_iter()
                .map(CoreLog::from)
                .collect(),
            gas_burned: mem::take(&mut self.gas_burned),
        }
    }

    pub(crate) fn process_tasks(&mut self, current_bn: u32) {
        let db_weights = DbWeights::default();

        let (first_incomplete_block, were_empty) = self
            .first_incomplete_tasks_block
            .take()
            .map(|block| {
                self.gas_allowance = self.gas_allowance.saturating_sub(db_weights.write.ref_time);
                (block, false)
            })
            .unwrap_or_else(|| {
                self.gas_allowance = self.gas_allowance.saturating_sub(db_weights.read.ref_time);
                (current_bn, true)
            });

        // When we had to stop processing due to insufficient gas allowance.
        let mut stopped_at = None;

        let missing_blocks = first_incomplete_block..=current_bn;
        for bn in missing_blocks {
            if self.gas_allowance <= db_weights.write.ref_time.saturating_mul(2) {
                stopped_at = Some(bn);
                log::debug!(
                    "Stopped processing tasks at: {stopped_at:?} due to insufficient allowance"
                );
                break;
            }

            let mut last_task = None;
            for task in self.task_pool.drain_prefix_keys(bn) {
                // decreasing allowance due to DB deletion
                self.on_task_pool_change();

                let max_task_gas = task::get_maximum_task_gas(&task);
                log::debug!(
                    "⚙️  Processing task {task:?} at the block {bn}, max gas = {max_task_gas}"
                );

                if self.gas_allowance.saturating_sub(max_task_gas) <= db_weights.write.ref_time {
                    // Since the task is not processed write DB cost should be refunded.
                    // In the same time gas allowance should be charged for read DB cost.
                    self.gas_allowance = self
                        .gas_allowance
                        .saturating_add(db_weights.write.ref_time)
                        .saturating_sub(db_weights.read.ref_time);

                    last_task = Some(task);

                    log::debug!("Not enough gas to process task at {bn:?}");

                    break;
                }

                let task_gas = task.process_with(self);

                self.gas_allowance = self.gas_allowance.saturating_sub(task_gas);

                if self.gas_allowance <= db_weights.write.ref_time + db_weights.read.ref_time {
                    stopped_at = Some(bn);
                    log::debug!("Stopping processing tasks at (read next): {stopped_at:?}");
                    break;
                }
            }

            if let Some(task) = last_task {
                stopped_at = Some(bn);

                self.gas_allowance = self.gas_allowance.saturating_add(db_weights.write.ref_time);

                self.task_pool.add(bn, task).unwrap_or_else(|e| {
                    let err_msg = format!(
                        "process_tasks: failed adding not processed last task to task pool. \
                        Bn - {bn:?}, task - {task:?}. Got error - {e:?}"
                    );

                    unreachable!("{err_msg}");
                });
                self.on_task_pool_change();
            }

            if stopped_at.is_some() {
                break;
            }
        }

        if let Some(stopped_at) = stopped_at {
            if were_empty {
                // Charging for inserting into storage of the first block of incomplete tasks,
                // if we were reading it only (they were empty).
                self.gas_allowance = self.gas_allowance.saturating_sub(db_weights.write.ref_time);
            }

            self.first_incomplete_tasks_block = Some(stopped_at);
        }
    }

    fn process_messages(&mut self) -> u32 {
        self.messages_processing_enabled = true;

        let block_config = self.block_config();

        log::debug!(
            "⚙️  Message queue processing at the block {}",
            self.block_height()
        );
        let mut total_processed = 0;
        while self.messages_processing_enabled {
            let dispatch = match self.dispatches.pop_front() {
                Some(dispatch) => dispatch,
                None => break,
            };

            let journal = self.process_dispatch(&block_config, dispatch);
            core_processor::handle_journal(journal, self);

            total_processed += 1;
        }

        total_processed
    }

    pub(crate) fn process_dispatch(
        &mut self,
        block_config: &BlockConfig,
        dispatch: StoredDispatch,
    ) -> Vec<JournalNote> {
        let destination_id = dispatch.destination();
        let dispatch_id = dispatch.id();
        let dispatch_kind = dispatch.kind();

        let gas_limit = self
            .gas_tree
            .get_limit(dispatch_id)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        log::debug!(
            "Processing message ({:?}): {:?} to {:?} / gas_limit: {}, gas_allowance: {}",
            dispatch_kind,
            dispatch_id,
            destination_id,
            gas_limit,
            self.gas_allowance,
        );

        if self.is_builtin(destination_id) {
            return self.process_builtin(dispatch, gas_limit);
        }

        let context = ContextCharged::new(
            destination_id,
            dispatch.into_incoming(gas_limit),
            self.gas_allowance,
        );

        let context = match context.charge_for_program(block_config) {
            Ok(context) => context,
            Err(journal) => {
                return journal;
            }
        };

        ProgramsStorageManager::modify_program(destination_id, |program| {
            let Some(program) = program else {
                log::debug!(
                    "Message {dispatch_id} is sent to program {destination_id} which does not exist"
                );
                return core_processor::process_code_not_exists(context);
            };

            match program.as_primary_program() {
                Program::Active(active_program) => {
                    // Check for invalid init message to already initialized program
                    if active_program.state == ProgramState::Initialized && dispatch_kind.is_init()
                    {
                        unreachable!(
                            "Got init message for already initialized program. \
                            Current init message id: {dispatch_id:?}, already initialized program id: {destination_id:?}"
                        );
                    }

                    // Handle uninitialized program states
                    if let ProgramState::Uninitialized { message_id } = active_program.state {
                        // Check for incomplete user programs (placeholder message ID)
                        if message_id == PLACEHOLDER_MESSAGE_ID {
                            unreachable!(
                                "Got message sent to incomplete user program. First send manually via `Program` API \
                                message to {destination_id} program, so it's completely created and possibly initialized."
                            );
                        }

                        // If the destination program is uninitialized, then we allow
                        // to process message, if it's a reply (async init case) or init message.
                        // Otherwise, we return error reply.
                        if message_id != dispatch_id && !dispatch_kind.is_reply() {
                            if dispatch_kind.is_init() {
                                // This should never happen as the protocol doesn't allow second
                                // init messages
                                unreachable!(
                                    "Got init message which is not the first init message to the program. \
                                    Current init message id: {dispatch_id:?}, original init message id: {message_id:?}, \
                                    program: {destination_id:?}"
                                );
                            }

                            return core_processor::process_uninitialized(context);
                        }
                    }
                }
                Program::Terminated(_) => {
                    log::debug!(
                        "Message {dispatch_id} is sent to program {destination_id} which is failed to initialize"
                    );
                    return core_processor::process_failed_init(context);
                }
                Program::Exited(inheritor) => {
                    log::debug!("Message {dispatch_id} is sent to exited program {destination_id}");
                    return core_processor::process_program_exited(context, *inheritor);
                }
            };

            // Dispatch to mock or regular program based on the type
            match program {
                GTestProgram::Default {
                    primary: Program::Active(active_program),
                } => self.process_program(
                    block_config,
                    context,
                    active_program,
                    dispatch_kind,
                    destination_id,
                ),
                GTestProgram::Mock { handlers, .. } => self.process_mock_program(context, handlers),
                _ => {
                    unreachable!("Program {destination_id:?} is in unexpected state - {program:?}");
                }
            }
        })
    }

    fn process_builtin(&mut self, dispatch: StoredDispatch, gas_limit: u64) -> Vec<JournalNote> {
        let destination = dispatch.destination();
        if !dispatch.kind().is_handle() {
            unreachable!(
                "Only handle dispatch kind is supported for built-in programs. Received: {:?}",
                dispatch.kind()
            );
        }

        if dispatch.context().is_some() {
            unreachable!("Built-in messages can't be executed more than 1 time");
        }

        // TODO #4832: Charge gas for built-in ops
        let mut mock_builtin_context = BuiltinContext::new(u64::MAX, u64::MAX);

        let builtin_call_res = match destination {
            BLS12_381_ID => bls12_381::execute_bls12_381_builtins::<
                BlsOpsGasCostsImpl,
                Bls12_381OpsLowLevel,
            >(dispatch.payload_bytes(), &mut mock_builtin_context)
            .map(|response| {
                log::debug!("BLS12-381 response: {response:?}");

                response
                    .encode()
                    .try_into()
                    .unwrap_or_else(|_| unreachable!("Failed to encode BLS12-381 builtin reply"))
            }),
            ETH_BRIDGE_ID => builtins::process_eth_bridge_dispatch(&dispatch).map(|response| {
                log::debug!("Eth-bridge response: {response:?}");

                response
                    .encode()
                    .try_into()
                    .unwrap_or_else(|_| unreachable!("Failed to encode eth-bridge builtin reply"))
            }),
            id => unimplemented!("Unknown builtin program id: {id}"),
        };

        let incoming_dispatch = dispatch.into_incoming(gas_limit);
        let gas_counter = GasCounter::new(gas_limit);

        // TODO #4832 Charge gas for built-in ops
        match builtin_call_res {
            Ok(reply_payload) => {
                let mut dispatch_result = DispatchResult::success(
                    &incoming_dispatch,
                    destination,
                    gas_counter.to_amount(),
                );

                // Create an artificial `MessageContext` object that will help us to generate
                // a reply from the builtin actor.
                // Dispatch clone is cheap here since it only contains Arc<Payload>
                let mut message_context =
                    MessageContext::new(incoming_dispatch.clone(), destination, Default::default());
                // TODO # , currently value is sent back
                let packet = ReplyPacket::new(reply_payload, incoming_dispatch.value());

                // Mark reply as sent
                if let Ok(_reply_id) = message_context.reply_commit(packet.clone(), None) {
                    let (outcome, context_store) = message_context.drain();

                    dispatch_result.context_store = context_store;
                    let ContextOutcomeDrain {
                        outgoing_dispatches: generated_dispatches,
                        ..
                    } = outcome.drain();
                    dispatch_result.generated_dispatches = generated_dispatches;
                    dispatch_result.reply_sent = true;
                } else {
                    unreachable!("Failed to send reply from builtin actor");
                };

                core_processor::process_success(
                    SuccessfulDispatchResultKind::Success,
                    dispatch_result,
                    incoming_dispatch,
                )
            }
            Err(BuiltinActorError::GasAllowanceExceeded) => {
                core_processor::process_allowance_exceed(incoming_dispatch, destination, 0)
            }
            Err(err) => {
                // Builtin actor call failed.
                log::debug!("Builtin actor error: {err:?}");
                let system_reservation_ctx =
                    SystemReservationContext::from_dispatch(&incoming_dispatch);
                core_processor::process_execution_error(
                    incoming_dispatch,
                    destination,
                    gas_counter.burned(),
                    system_reservation_ctx,
                    builtin_error_into_actor_error(err),
                )
            }
        }
    }

    fn process_program(
        &mut self,
        block_config: &BlockConfig,
        context: ContextCharged<ForProgram>,
        active_program: &mut ActiveProgram<BlockNumber>,
        dispatch_kind: DispatchKind,
        destination_id: ActorId,
    ) -> Vec<JournalNote> {
        let context = match context.charge_for_code_metadata(block_config) {
            Ok(context) => context,
            Err(journal) => {
                return journal;
            }
        };

        let code_id = active_program.code_id;
        let code_metadata = self.code_metadata(code_id).cloned().unwrap_or_else(|| {
            unreachable!(
                "Code metadata for program {destination_id:?} with code id {} not found",
                active_program.code_id
            )
        });

        // TODO: This is an early check, it should be moved after re-instrumentation, in
        // case re-instrumentation will change/add exports
        if !code_metadata.exports().contains(&dispatch_kind) {
            let (destination_id, dispatch, gas_counter, _) = context.into_parts();

            let notes = core_processor::process_success(
                SuccessfulDispatchResultKind::Success,
                DispatchResult::success(&dispatch, destination_id, gas_counter.to_amount()),
                dispatch,
            );

            return notes;
        }

        // No re-instrumentation is needed for test environment,
        // as `gtest` test runtime doesn't provide an opportunity
        // to change the Schedule with weights data.
        let instrumented_code_len = code_metadata.instrumented_code_len().unwrap_or_else(|| {
            let err_msg = format!(
                "Code metadata for the existing program does not contain \
                instrumented code length. Program id -'{destination_id:?}', Code id - '{code_id:?}'."
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });

        // Adjust gas counters for fetching instrumented binary code.
        let context =
            match context.charge_for_instrumented_code(block_config, instrumented_code_len) {
                Ok(context) => context,
                Err(journal) => return journal,
            };

        let instrumented_code = self.instrumented_code(code_id).cloned().unwrap_or_else(|| {
            let err_msg = format!(
                "Failed to get instrumented code for the existing program. \
                    Program id -'{destination_id:?}', Code id - '{code_id:?}'."
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });

        let context = match context
            .charge_for_allocations(block_config, active_program.allocations_tree_len)
        {
            Ok(context) => context,
            Err(journal) => {
                return journal;
            }
        };

        let allocations = if active_program.allocations_tree_len != 0 {
            ProgramsStorageManager::allocations(destination_id).unwrap_or_else(||
                unreachable!(
                    "`allocations_tree_len` {} is not zero, so program {destination_id:?} must have allocations",
                    active_program.allocations_tree_len,
                )
            )
        } else {
            Default::default()
        };

        let actor_data = ExecutableActorData {
            allocations,
            memory_infix: active_program.memory_infix,
            gas_reservation_map: active_program.gas_reservation_map.clone(),
        };

        let context = match context.charge_for_module_instantiation(
            block_config,
            actor_data,
            instrumented_code.instantiated_section_sizes(),
            &code_metadata,
        ) {
            Ok(context) => context,
            Err(journal) => {
                return journal;
            }
        };

        let balance = Accounts::reducible_balance(destination_id);

        core_processor::process::<Ext<LazyPagesNative>>(
            block_config,
            ProcessExecutionContext::new(
                context,
                InstrumentedCodeAndMetadata {
                    instrumented_code,
                    metadata: code_metadata,
                },
                balance,
            ),
            (
                blocks::current_epoch_random(),
                block_config.block_info.height,
            ),
        )
        .unwrap_or_else(|e| unreachable!("core-processor logic violated: {}", e))
    }

    fn process_mock_program(
        &mut self,
        context: ContextCharged<ForProgram>,
        handlers: &mut Box<dyn WasmProgram>,
    ) -> Vec<JournalNote> {
        let (destination_id, dispatch, gas_counter, _) = context.into_parts();
        let payload = dispatch.payload().to_vec();
        let dispatch_kind = dispatch.kind();

        let outcome = match dispatch_kind {
            DispatchKind::Init => {
                log::debug!("Calling mock program init for {destination_id:?}");
                handlers.init(payload)
            }
            DispatchKind::Handle => {
                log::debug!("Calling mock program handle for {destination_id:?}");
                handlers.handle(payload)
            }
            kind => unreachable!("Unsupported dispatch kind {kind:?} for mock program"),
        };

        match outcome {
            Ok(maybe_reply) => {
                let (generated_dispatches, reply_sent) = maybe_reply
                    .map(|payload| {
                        let reply_message = ReplyMessage::from_packet(
                            MessageId::generate_reply(dispatch.id()),
                            ReplyPacket::new(payload.try_into().expect("too big payload"), 0),
                        );
                        let dispatch = reply_message.into_dispatch(
                            destination_id,
                            dispatch.source(),
                            dispatch.id(),
                        );

                        (vec![(dispatch, 0, None)], true)
                    })
                    .unwrap_or_default();

                let dispatch_result = {
                    let mut dp =
                        DispatchResult::success(&dispatch, destination_id, gas_counter.to_amount());
                    dp.generated_dispatches = generated_dispatches;
                    dp.reply_sent = reply_sent;

                    dp
                };

                core_processor::process_success(
                    SuccessfulDispatchResultKind::Success,
                    dispatch_result,
                    dispatch,
                )
            }
            Err(error_msg) => {
                handlers.debug(error_msg);

                let err = ActorExecutionErrorReplyReason::Trap(TrapExplanation::Panic(
                    LimitedStr::from_small_str(error_msg).into(),
                ));

                core_processor::process_execution_error(
                    dispatch,
                    destination_id,
                    gas_counter.burned(),
                    Default::default(),
                    err,
                )
            }
        }
    }

    pub(crate) fn block_config(&self) -> BlockConfig {
        let schedule = Schedule::default();
        BlockConfig {
            block_info: self.blocks_manager.get(),
            performance_multiplier: gsys::Percent::new(100),
            forbidden_funcs: Default::default(),
            reserve_for: RESERVE_FOR,
            gas_multiplier: gsys::GasMultiplier::from_value_per_gas(VALUE_PER_GAS),
            costs: schedule.process_costs(),
            existential_deposit: EXISTENTIAL_DEPOSIT,
            mailbox_threshold: schedule.rent_weights.mailbox_threshold.ref_time,
            max_reservations: MAX_RESERVATIONS,
            max_pages: MAX_WASM_PAGES_AMOUNT.into(),
            outgoing_limit: OUTGOING_LIMIT,
            outgoing_bytes_limit: OUTGOING_BYTES_LIMIT,
        }
    }
}

fn builtin_error_into_actor_error(err: BuiltinActorError) -> ActorExecutionErrorReplyReason {
    match err {
        BuiltinActorError::InsufficientGas => {
            ActorExecutionErrorReplyReason::Trap(TrapExplanation::GasLimitExceeded)
        }
        BuiltinActorError::InsufficientValue => ActorExecutionErrorReplyReason::Trap(
            TrapExplanation::Panic(LimitedStr::from_small_str("Not enough value supplied").into()),
        ),
        BuiltinActorError::DecodingError => ActorExecutionErrorReplyReason::Trap(
            TrapExplanation::Panic(LimitedStr::from_small_str("Message decoding error").into()),
        ),
        BuiltinActorError::Custom(e) => {
            ActorExecutionErrorReplyReason::Trap(TrapExplanation::Panic(e.into()))
        }
        BuiltinActorError::GasAllowanceExceeded => {
            unreachable!("Never supposed to be converted to error reply reason")
        }
        BuiltinActorError::EmptyG1PointsList => ActorExecutionErrorReplyReason::Trap(
            TrapExplanation::Panic(LimitedStr::from_small_str("Empty G1 points list").into()),
        ),
        BuiltinActorError::MapperCreationError => ActorExecutionErrorReplyReason::Trap(
            TrapExplanation::Panic(LimitedStr::from_small_str("Mapper creation error").into()),
        ),
        BuiltinActorError::MessageMappingError => ActorExecutionErrorReplyReason::Trap(
            TrapExplanation::Panic(LimitedStr::from_small_str("Message mapping error").into()),
        ),
    }
}
