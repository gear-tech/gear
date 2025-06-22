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

use gear_core::code::MAX_WASM_PAGES_AMOUNT;
use task::get_maximum_task_gas;

use super::*;

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

        if Actors::is_program(source) {
            usage_panic!(
                "Sending messages allowed only from users id. Please, provide user id as source."
            );
        }

        // User must exist
        if !Accounts::exists(source) {
            usage_panic!("User's {source} balance is zero; mint value to it first.");
        }

        if !Actors::is_active_program(destination) {
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
        if Actors::is_user(stored_dispatch.destination()) {
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

                let max_task_gas = get_maximum_task_gas(&task);
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

                self.task_pool.add(bn, task.clone()).unwrap_or_else(|e| {
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

            self.process_dispatch(&block_config, dispatch);

            total_processed += 1;
        }

        total_processed
    }

    fn process_dispatch(&mut self, block_config: &BlockConfig, dispatch: StoredDispatch) {
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

        let balance = Accounts::reducible_balance(destination_id);

        let context = match core_processor::precharge_for_program(
            block_config,
            self.gas_allowance,
            dispatch.into_incoming(gas_limit),
            destination_id,
        ) {
            Ok(dispatch) => dispatch,
            Err(journal) => {
                core_processor::handle_journal(journal, self);
                return;
            }
        };

        #[allow(clippy::large_enum_variant)]
        enum Exec {
            Notes(Vec<JournalNote>),
            ExecutableActor(
                (ExecutableActorData, InstrumentedCode),
                ContextChargedForProgram,
            ),
        }

        let exec = Actors::modify(destination_id, |actor| {
            use TestActor::*;

            let actor = actor.unwrap_or_else(|| unreachable!("actor must exist for queue message"));

            match actor {
                Initialized(_) => {}
                Uninitialized(_, _) => { /* uninit case is checked further */ }
                FailedInit => {
                    log::debug!(
                        "Message {dispatch_id} is sent to program {destination_id} which is failed to initialize"
                    );
                    return Exec::Notes(core_processor::process_failed_init(context));
                }
                CodeNotExists => {
                    log::debug!(
                        "Message {dispatch_id} is sent to program {destination_id} which code does not exist"
                    );
                    return Exec::Notes(core_processor::process_code_not_exists(context));
                }
                Exited(inheritor) => {
                    log::debug!("Message {dispatch_id} is sent to exited program {destination_id}");
                    return Exec::Notes(core_processor::process_program_exited(
                        context, *inheritor,
                    ));
                }
            }

            if actor.is_initialized() && dispatch_kind.is_init() {
                // Panic is impossible, because gear protocol does not provide functionality
                // to send second init message to any already existing program.
                let err_msg = format!(
                    "Got init message for already initialized program. \
                    Current init message id - {dispatch_id:?}, already initialized program id - {destination_id:?}."
                );

                unreachable!("{err_msg}");
            }

            if matches!(actor, Uninitialized(None, _)) {
                let err_msg = format!(
                    "Got message sent to incomplete user program. First send manually via `Program` API message \
                    to {destination_id} program, so it's completely created and possibly initialized."
                );

                unreachable!("{err_msg}");
            }

            // If the destination program is uninitialized, then we allow
            // to process message, if it's a reply or init message.
            // Otherwise, we return error reply.
            if matches!(actor, Uninitialized(Some(message_id), _)
                if *message_id != dispatch_id && !dispatch_kind.is_reply())
            {
                if dispatch_kind.is_init() {
                    // Panic is impossible, because gear protocol does not provide functionality
                    // to send second init message to any existing program.
                    let err_msg = format!(
                        "run_queue_step: got init message which is not the first init message to the program. \
                        Current init message id - {dispatch_id:?}, original init message id - {dispatch_id}, program - {destination_id:?}.",
                    );

                    unreachable!("{err_msg}");
                }

                return Exec::Notes(core_processor::process_uninitialized(context));
            }

            if let Some(data) = actor.get_executable_actor_data() {
                Exec::ExecutableActor(data, context)
            } else {
                unreachable!("invalid program state");
            }
        });

        let journal = match exec {
            Exec::Notes(journal) => journal,
            Exec::ExecutableActor((actor_data, instrumented_code), context) => self
                .process_executable_actor(
                    actor_data,
                    instrumented_code,
                    block_config,
                    context,
                    balance,
                ),
        };

        core_processor::handle_journal(journal, self)
    }

    fn process_executable_actor(
        &self,
        actor_data: ExecutableActorData,
        instrumented_code: InstrumentedCode,
        block_config: &BlockConfig,
        context: ContextChargedForProgram,
        balance: Value,
    ) -> Vec<JournalNote> {
        let context = match core_processor::precharge_for_allocations(
            block_config,
            context,
            actor_data.allocations.intervals_amount() as u32,
        ) {
            Ok(context) => context,
            Err(journal) => {
                return journal;
            }
        };

        let context =
            match core_processor::precharge_for_code_length(block_config, context, actor_data) {
                Ok(context) => context,
                Err(journal) => {
                    return journal;
                }
            };

        let code_id = context.actor_data().code_id;
        let code_len_bytes = self
            .read_code(code_id)
            .map(|code| code.len().try_into().expect("too big code len"))
            .unwrap_or_else(|| unreachable!("can't find code for the existing code id {code_id}"));
        let context =
            match core_processor::precharge_for_code(block_config, context, code_len_bytes) {
                Ok(context) => context,
                Err(journal) => {
                    return journal;
                }
            };

        let context = match core_processor::precharge_for_module_instantiation(
            block_config,
            // No re-instrumentation
            ContextChargedForInstrumentation::from(context),
            instrumented_code.instantiated_section_sizes(),
        ) {
            Ok(context) => context,
            Err(journal) => {
                return journal;
            }
        };

        core_processor::process::<Ext<LazyPagesNative>>(
            block_config,
            (context, instrumented_code, balance).into(),
            self.random_data.clone(),
        )
        .unwrap_or_else(|e| unreachable!("core-processor logic violated: {}", e))
    }

    fn block_config(&self) -> BlockConfig {
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
