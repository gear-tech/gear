// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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

    #[track_caller]
    fn validate_dispatch(&mut self, dispatch: &Dispatch) {
        let source = dispatch.source();
        let destination = dispatch.destination();

        if Actors::is_program(source) {
            panic!("Sending messages allowed only from users id");
        }

        if dispatch.is_reply() && !Actors::is_active_program(destination) {
            panic!("Can't send reply to a non-active program {destination:?}");
        }

        // User must exist
        if !Accounts::exists(source) {
            panic!("User's {source} balance is zero; mint value to it first.");
        }

        let is_init_msg = dispatch.kind().is_init();
        // We charge ED only for init messages
        let maybe_ed = if is_init_msg { EXISTENTIAL_DEPOSIT } else { 0 };
        let balance = Accounts::balance(source);

        let gas_limit = dispatch
            .gas_limit()
            .unwrap_or_else(|| unreachable!("message from program API always has gas"));
        let gas_value = GAS_MULTIPLIER.gas_to_value(gas_limit);

        // Check sender has enough balance to cover dispatch costs
        if balance < { dispatch.value() + gas_value + maybe_ed } {
            panic!(
                "Insufficient balance: user ({}) tries to send \
                ({}) value, ({}) gas and ED ({}), while his balance ({:?})",
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

        if dispatch.value() != 0 {
            // Deposit message value
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

    // TODO #4120 Charge for task pool processing the gas from gas allowance
    // TODO #4121
    #[track_caller]
    pub(crate) fn run_new_block(&mut self, allowance: Gas) -> BlockRunResult {
        self.gas_allowance = allowance;
        self.blocks_manager.next_block();
        let new_block_bn = self.block_height();

        self.process_tasks(new_block_bn);
        let total_processed = self.process_messages();

        BlockRunResult {
            block_info: self.blocks_manager.get(),
            gas_allowance_spent: Gas(GAS_ALLOWANCE) - self.gas_allowance,
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

    #[track_caller]
    pub(crate) fn process_tasks(&mut self, bn: u32) {
        for task in self.task_pool.drain_prefix_keys(bn) {
            task.process_with(self);
        }
    }

    #[track_caller]
    fn process_messages(&mut self) -> u32 {
        self.messages_processing_enabled = true;

        let mut total_processed = 0;
        while self.messages_processing_enabled {
            let dispatch = match self.dispatches.pop_front() {
                Some(dispatch) => dispatch,
                None => break,
            };

            enum DispatchCase {
                Dormant,
                Normal(ExecutableActorData, InstrumentedCode),
                Mock(Box<dyn WasmProgram>),
            }

            let dispatch_case = Actors::modify(dispatch.destination(), |actor| {
                let actor = actor
                    .unwrap_or_else(|| panic!("Somehow message queue contains message for user"));
                if actor.is_dormant() {
                    DispatchCase::Dormant
                } else if let Some((data, code)) = actor.get_executable_actor_data() {
                    DispatchCase::Normal(data, code)
                } else if let Some(mock) = actor.take_mock() {
                    DispatchCase::Mock(mock)
                } else {
                    unreachable!();
                }
            });
            let balance = Accounts::reducible_balance(dispatch.destination());

            match dispatch_case {
                DispatchCase::Dormant => self.process_dormant(balance, dispatch),
                DispatchCase::Normal(data, code) => {
                    self.process_normal(balance, data, code, dispatch)
                }
                DispatchCase::Mock(mock) => self.process_mock(mock, dispatch),
            }

            total_processed += 1;
        }

        total_processed
    }

    fn process_mock(&mut self, mut mock: Box<dyn WasmProgram>, dispatch: StoredDispatch) {
        enum Mocked {
            Reply(Option<Vec<u8>>),
            Signal,
        }

        let message_id = dispatch.id();
        let source = dispatch.source();
        let program_id = dispatch.destination();
        let payload = dispatch.payload_bytes().to_vec();

        let response = match dispatch.kind() {
            DispatchKind::Init => mock.init(payload).map(Mocked::Reply),
            DispatchKind::Handle => mock.handle(payload).map(Mocked::Reply),
            DispatchKind::Reply => mock.handle_reply(payload).map(|_| Mocked::Reply(None)),
            DispatchKind::Signal => mock.handle_signal(payload).map(|_| Mocked::Signal),
        };

        match response {
            Ok(Mocked::Reply(reply)) => {
                let maybe_reply_message = if let Some(payload) = reply {
                    let id = MessageId::generate_reply(message_id);
                    let packet = ReplyPacket::new(payload.try_into().unwrap(), 0);
                    Some(ReplyMessage::from_packet(id, packet))
                } else {
                    (!dispatch.is_reply() && dispatch.kind() != DispatchKind::Signal)
                        .then_some(ReplyMessage::auto(message_id))
                };

                if let Some(reply_message) = maybe_reply_message {
                    <Self as JournalHandler>::send_dispatch(
                        self,
                        message_id,
                        reply_message.into_dispatch(program_id, dispatch.source(), message_id),
                        0,
                        None,
                    );
                }

                if let DispatchKind::Init = dispatch.kind() {
                    self.message_dispatched(
                        message_id,
                        source,
                        DispatchOutcome::InitSuccess { program_id },
                    );
                }
            }
            Ok(Mocked::Signal) => {}
            Err(expl) => {
                mock.debug(expl);

                if let DispatchKind::Init = dispatch.kind() {
                    self.message_dispatched(
                        message_id,
                        source,
                        DispatchOutcome::InitFailure {
                            program_id,
                            origin: source,
                            reason: expl.to_string(),
                        },
                    );
                } else {
                    self.message_dispatched(
                        message_id,
                        source,
                        DispatchOutcome::MessageTrap {
                            program_id,
                            trap: expl.to_string(),
                        },
                    )
                }

                if !dispatch.is_reply() && dispatch.kind() != DispatchKind::Signal {
                    let err = ErrorReplyReason::Execution(SimpleExecutionError::UserspacePanic);
                    let err_payload = expl
                        .as_bytes()
                        .to_vec()
                        .try_into()
                        .unwrap_or_else(|_| unreachable!("Error message is too large"));

                    let reply_message = ReplyMessage::system(message_id, err_payload, err);

                    <Self as JournalHandler>::send_dispatch(
                        self,
                        message_id,
                        reply_message.into_dispatch(program_id, dispatch.source(), message_id),
                        0,
                        None,
                    );
                }
            }
        }

        // After run either `init_success` is called or `init_failed`.
        // So only active (init success) program can be modified
        Actors::modify(program_id, |actor| {
            if let Some(TestActor::Initialized(old_mock)) = actor {
                *old_mock = Program::Mock(Some(mock));
            }
        })
    }

    fn process_normal(
        &mut self,
        balance: u128,
        data: ExecutableActorData,
        code: InstrumentedCode,
        dispatch: StoredDispatch,
    ) {
        self.process_dispatch(balance, Some((data, code)), dispatch);
    }

    fn process_dormant(&mut self, balance: u128, dispatch: StoredDispatch) {
        self.process_dispatch(balance, None, dispatch);
    }

    #[track_caller]
    fn process_dispatch(
        &mut self,
        balance: u128,
        data: Option<(ExecutableActorData, InstrumentedCode)>,
        dispatch: StoredDispatch,
    ) {
        let dest = dispatch.destination();
        let gas_limit = self
            .gas_tree
            .get_limit(dispatch.id())
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
        let schedule = Schedule::default();
        let block_config = BlockConfig {
            block_info: self.blocks_manager.get(),
            performance_multiplier: gsys::Percent::new(100),
            forbidden_funcs: Default::default(),
            reserve_for: RESERVE_FOR,
            gas_multiplier: gsys::GasMultiplier::from_value_per_gas(VALUE_PER_GAS),
            costs: schedule.process_costs(),
            existential_deposit: EXISTENTIAL_DEPOSIT,
            mailbox_threshold: MAILBOX_THRESHOLD,
            max_reservations: MAX_RESERVATIONS,
            max_pages: TESTS_MAX_PAGES_NUMBER.into(),
            outgoing_limit: OUTGOING_LIMIT,
            outgoing_bytes_limit: OUTGOING_BYTES_LIMIT,
        };

        let context = match core_processor::precharge_for_program(
            &block_config,
            self.gas_allowance.0,
            dispatch.into_incoming(gas_limit),
            dest,
        ) {
            Ok(d) => d,
            Err(journal) => {
                core_processor::handle_journal(journal, self);
                return;
            }
        };

        let Some((actor_data, code)) = data else {
            let journal = core_processor::process_non_executable(context);
            core_processor::handle_journal(journal, self);
            return;
        };

        let context = match core_processor::precharge_for_allocations(
            &block_config,
            context,
            actor_data.allocations.intervals_amount() as u32,
        ) {
            Ok(c) => c,
            Err(journal) => {
                core_processor::handle_journal(journal, self);
                return;
            }
        };

        let context =
            match core_processor::precharge_for_code_length(&block_config, context, actor_data) {
                Ok(c) => c,
                Err(journal) => {
                    core_processor::handle_journal(journal, self);
                    return;
                }
            };

        let context = ContextChargedForCode::from(context);
        let context = ContextChargedForInstrumentation::from(context);
        let context = match core_processor::precharge_for_module_instantiation(
            &block_config,
            context,
            code.instantiated_section_sizes(),
        ) {
            Ok(c) => c,
            Err(journal) => {
                core_processor::handle_journal(journal, self);
                return;
            }
        };

        let journal = core_processor::process::<Ext<LazyPagesNative>>(
            &block_config,
            (context, code, balance).into(),
            self.random_data.clone(),
        )
        .unwrap_or_else(|e| unreachable!("core-processor logic violated: {}", e));

        core_processor::handle_journal(journal, self);
    }
}
