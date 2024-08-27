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

mod hold_bound;
mod journal;
mod reservations;
mod task;

use crate::{
    accounts::Accounts,
    actors::{Actors, GenuineProgram, Program, TestActor},
    bank::Bank,
    blocks::BlocksManager,
    constants::Value,
    gas_tree::GasTreeManager,
    log::{BlockRunResult, CoreLog},
    mailbox::MailboxManager,
    program::{Gas, WasmProgram},
    task_pool::TaskPoolManager,
    waitlist::WaitlistManager,
    Result, TestError, DISPATCH_HOLD_COST, EPOCH_DURATION_IN_BLOCKS, EXISTENTIAL_DEPOSIT,
    GAS_ALLOWANCE, GAS_MULTIPLIER, HOST_FUNC_READ_COST, HOST_FUNC_WRITE_AFTER_READ_COST,
    HOST_FUNC_WRITE_COST, INITIAL_RANDOM_SEED, LOAD_ALLOCATIONS_PER_INTERVAL,
    LOAD_PAGE_STORAGE_DATA_COST, MAILBOX_COST, MAILBOX_THRESHOLD, MAX_RESERVATIONS,
    MODULE_CODE_SECTION_INSTANTIATION_BYTE_COST, MODULE_DATA_SECTION_INSTANTIATION_BYTE_COST,
    MODULE_ELEMENT_SECTION_INSTANTIATION_BYTE_COST, MODULE_GLOBAL_SECTION_INSTANTIATION_BYTE_COST,
    MODULE_INSTRUMENTATION_BYTE_COST, MODULE_INSTRUMENTATION_COST,
    MODULE_TABLE_SECTION_INSTANTIATION_BYTE_COST, MODULE_TYPE_SECTION_INSTANTIATION_BYTE_COST,
    READ_COST, READ_PER_BYTE_COST, RESERVATION_COST, RESERVE_FOR, SIGNAL_READ_COST,
    SIGNAL_WRITE_AFTER_READ_COST, SIGNAL_WRITE_COST, VALUE_PER_GAS, WAITLIST_COST, WRITE_COST,
};
use core_processor::{
    common::*,
    configs::{
        BlockConfig, ExtCosts, InstantiationCosts, ProcessCosts, RentCosts, TESTS_MAX_PAGES_NUMBER,
    },
    ContextChargedForCode, ContextChargedForInstrumentation, Ext,
};
use gear_common::{
    auxiliary::{
        gas_provider::PlainNodeId, mailbox::MailboxErrorImpl, waitlist::WaitlistErrorImpl,
        BlockNumber,
    },
    event::{MessageWaitedReason, MessageWaitedRuntimeReason},
    scheduler::{ScheduledTask, StorageType},
    storage::Interval,
    LockId, Origin,
};
use gear_core::{
    code::{Code, CodeAndId, InstrumentedCode, InstrumentedCodeAndId, TryNewCodeConfig},
    ids::{prelude::*, CodeId, MessageId, ProgramId, ReservationId},
    memory::PageBuf,
    message::{
        Dispatch, DispatchKind, Message, ReplyMessage, ReplyPacket, StoredDelayedDispatch,
        StoredDispatch, StoredMessage, UserMessage, UserStoredMessage,
    },
    pages::{num_traits::Zero, GearPage},
};
use gear_core_errors::{ErrorReplyReason, SimpleExecutionError};
use gear_lazy_pages_common::LazyPagesCosts;
use gear_lazy_pages_native_interface::LazyPagesNative;
use hold_bound::HoldBoundBuilder;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    convert::TryInto,
    fmt::Debug,
    mem,
};

const OUTGOING_LIMIT: u32 = 1024;
const OUTGOING_BYTES_LIMIT: u32 = 64 * 1024 * 1024;

#[derive(Debug, Default)]
pub(crate) struct ExtManager {
    // State metadata
    pub(crate) blocks_manager: BlocksManager,
    pub(crate) random_data: (Vec<u8>, u32),

    // Messaging and programs meta
    pub(crate) msg_nonce: u64,
    pub(crate) id_nonce: u64,

    // State
    pub(crate) bank: Bank,
    pub(crate) opt_binaries: BTreeMap<CodeId, Vec<u8>>,
    pub(crate) meta_binaries: BTreeMap<CodeId, Vec<u8>>,
    pub(crate) dispatches: VecDeque<StoredDispatch>,
    pub(crate) mailbox: MailboxManager,
    pub(crate) task_pool: TaskPoolManager,
    pub(crate) waitlist: WaitlistManager,
    pub(crate) gas_tree: GasTreeManager,
    pub(crate) gas_allowance: Gas,
    pub(crate) dispatches_stash: HashMap<MessageId, (StoredDelayedDispatch, Interval<BlockNumber>)>,
    pub(crate) messages_processing_enabled: bool,

    // Last block execution info
    pub(crate) succeed: BTreeSet<MessageId>,
    pub(crate) failed: BTreeSet<MessageId>,
    pub(crate) not_executed: BTreeSet<MessageId>,
    pub(crate) gas_burned: BTreeMap<MessageId, Gas>,
    pub(crate) log: Vec<StoredMessage>,
}

impl ExtManager {
    #[track_caller]
    pub(crate) fn new() -> Self {
        Self {
            msg_nonce: 1,
            id_nonce: 1,
            blocks_manager: BlocksManager::new(),
            messages_processing_enabled: true,
            random_data: (
                {
                    let mut rng = StdRng::seed_from_u64(INITIAL_RANDOM_SEED);
                    let mut random = [0u8; 32];
                    rng.fill_bytes(&mut random);

                    random.to_vec()
                },
                0,
            ),
            ..Default::default()
        }
    }

    pub fn block_height(&self) -> u32 {
        self.blocks_manager.get().height
    }

    pub(crate) fn store_new_actor(
        &mut self,
        program_id: ProgramId,
        program: Program,
        init_message_id: Option<MessageId>,
    ) -> Option<TestActor> {
        if let Program::Genuine(GenuineProgram { code, .. }) = &program {
            self.store_new_code(code.code().to_vec());
        }
        Actors::insert(program_id, TestActor::new(init_message_id, program))
    }

    pub(crate) fn store_new_code(&mut self, code: Vec<u8>) -> CodeId {
        let code_id = CodeId::generate(&code);
        self.opt_binaries.insert(code_id, code);
        code_id
    }

    pub(crate) fn read_code(&self, code_id: CodeId) -> Option<&[u8]> {
        self.opt_binaries.get(&code_id).map(Vec::as_slice)
    }

    pub(crate) fn fetch_inc_message_nonce(&mut self) -> u64 {
        let nonce = self.msg_nonce;
        self.msg_nonce += 1;
        nonce
    }

    pub(crate) fn free_id_nonce(&mut self) -> u64 {
        while Actors::contains_key(self.id_nonce.into()) {
            self.id_nonce += 1;
        }
        self.id_nonce
    }

    /// Insert message into the delayed queue.
    pub(crate) fn send_delayed_dispatch(
        &mut self,
        origin_msg: MessageId,
        dispatch: Dispatch,
        delay: u32,
        to_user: bool,
        reservation: Option<ReservationId>,
    ) {
        if delay.is_zero() {
            let err_msg = "send_delayed_dispatch: delayed sending with zero delay appeared";

            unreachable!("{err_msg}");
        }

        let message_id = dispatch.id();

        if self.dispatches_stash.contains_key(&message_id) {
            let err_msg = format!(
                "send_delayed_dispatch: stash already has the message id - {id}",
                id = dispatch.id()
            );

            unreachable!("{err_msg}");
        }

        // Validating dispatch wasn't sent from system with delay.
        if dispatch.is_error_reply() || matches!(dispatch.kind(), DispatchKind::Signal) {
            let err_msg = format!(
                "send_delayed_dispatch: message of an invalid kind is sent: {kind:?}",
                kind = dispatch.kind()
            );

            unreachable!("{err_msg}");
        }

        let mut to_mailbox = false;

        let sender_node = reservation
            .map(Origin::into_origin)
            .unwrap_or_else(|| origin_msg.into_origin());

        let from = dispatch.source();
        let value = dispatch.value();

        let hold_builder = HoldBoundBuilder::new(StorageType::DispatchStash);

        let delay_hold = hold_builder.duration(self, delay);
        let gas_for_delay = delay_hold.lock_amount(self);

        let interval_finish = if to_user {
            let threshold = MAILBOX_THRESHOLD;

            let gas_limit = dispatch
                .gas_limit()
                .or_else(|| {
                    let gas_limit = self.gas_tree.get_limit(sender_node).unwrap_or_else(|e| {
                        let err_msg = format!(
                            "send_delayed_dispatch: failed getting message gas limit. \
                                Lock sponsor id - {sender_node:?}. Got error - {e:?}"
                        );

                        unreachable!("{err_msg}");
                    });

                    (gas_limit.saturating_sub(gas_for_delay) >= threshold).then_some(threshold)
                })
                .unwrap_or_default();

            to_mailbox = !dispatch.is_reply() && gas_limit >= threshold;

            let gas_amount = if to_mailbox {
                gas_for_delay.saturating_add(gas_limit)
            } else {
                gas_for_delay
            };

            self.gas_tree
                .cut(sender_node, message_id, gas_amount)
                .unwrap_or_else(|e| {
                    let sender_node = sender_node.cast::<PlainNodeId>();
                    let err_msg = format!(
                        "send_delayed_dispatch: failed creating cut node. \
                        Origin node - {sender_node:?}, cut node id - {id}, amount - {gas_amount}. \
                        Got error - {e:?}",
                        id = dispatch.id()
                    );

                    unreachable!("{err_msg}");
                });

            if !to_mailbox {
                self.gas_tree
                    .split_with_value(
                        true,
                        origin_msg,
                        MessageId::generate_reply(dispatch.id()),
                        0,
                    )
                    .expect("failed to split with value gas node");
            }

            if let Some(reservation_id) = reservation {
                self.remove_gas_reservation_with_task(dispatch.source(), reservation_id)
            }

            // Locking funds for holding.
            let lock_id = delay_hold.lock_id().unwrap_or_else(|| {
                // Dispatch stash storage is guaranteed to have an associated lock id
                let err_msg =
                    "send_delayed_dispatch: No associated lock id for the dispatch stash storage";

                unreachable!("{err_msg}");
            });

            self.gas_tree.lock(dispatch.id(), lock_id, delay_hold.lock_amount(self))
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_delayed_dispatch: failed locking gas for the user message stash hold. \
                        Message id - {message_id}, lock amount - {lock}. Got error - {e:?}",
                        message_id = dispatch.id(),
                        lock = delay_hold.lock_amount(self));
                    unreachable!("{err_msg}");
                });

            if delay_hold.expected_duration(self).is_zero() {
                let err_msg = format!(
                    "send_delayed_dispatch: user message got zero duration hold bound for dispatch stash. \
                    Requested duration - {delay}, block cost - {cost}, source - {from:?}",
                    cost = Self::cost_by_storage_type(StorageType::DispatchStash)
                );

                unreachable!("{err_msg}");
            }

            delay_hold.expected()
        } else {
            match (dispatch.gas_limit(), reservation) {
                (Some(gas_limit), None) => self
                    .gas_tree
                    .split_with_value(
                        dispatch.is_reply(),
                        sender_node,
                        dispatch.id(),
                        gas_limit.saturating_add(gas_for_delay),
                    )
                    .expect("GasTree corrupted"),

                (None, None) => self
                    .gas_tree
                    .split(dispatch.is_reply(), sender_node, dispatch.id())
                    .expect("GasTree corrupted"),
                (Some(gas_limit), Some(reservation_id)) => {
                    let err_msg = format!(
                        "send_delayed_dispatch: sending dispatch with gas from reservation isn't implemented. \
                        Message - {message_id}, sender - {sender}, gas limit - {gas_limit}, reservation - {reservation_id}",
                        message_id = dispatch.id(),
                        sender = dispatch.source(),
                    );

                    unreachable!("{err_msg}");
                }

                (None, Some(reservation_id)) => {
                    self.gas_tree
                        .split(dispatch.is_reply(), reservation_id, dispatch.id())
                        .expect("GasTree corrupted");
                    self.remove_gas_reservation_with_task(dispatch.source(), reservation_id);
                }
            }

            let lock_id = delay_hold.lock_id().unwrap_or_else(|| {
                // Dispatch stash storage is guaranteed to have an associated lock id
                let err_msg =
                    "send_delayed_dispatch: No associated lock id for the dispatch stash storage";

                unreachable!("{err_msg}");
            });

            self.gas_tree
                .lock(dispatch.id(), lock_id, delay_hold.lock_amount(self))
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                    "send_delayed_dispatch: failed locking gas for the program message stash hold. \
                    Message id - {message_id}, lock amount - {lock}. Got error - {e:?}",
                    message_id = dispatch.id(),
                    lock = delay_hold.lock_amount(self)
                );

                    unreachable!("{err_msg}");
                });

            if delay_hold.expected_duration(self).is_zero() {
                let err_msg = format!(
                    "send_delayed_dispatch: program message got zero duration hold bound for dispatch stash. \
                    Requested duration - {delay}, block cost - {cost}, source - {from:?}",
                    cost = Self::cost_by_storage_type(StorageType::DispatchStash)
                );

                unreachable!("{err_msg}");
            }

            delay_hold.expected()
        };

        if !dispatch.value().is_zero() {
            self.bank.deposit_value(from, value, false);
        }

        let message_id = dispatch.id();

        let start_bn = self.block_height();
        let delay_interval = Interval {
            start: start_bn,
            finish: interval_finish,
        };

        self.dispatches_stash
            .insert(message_id, (dispatch.into_stored_delayed(), delay_interval));

        let task = if to_user {
            ScheduledTask::SendUserMessage {
                message_id,
                to_mailbox,
            }
        } else {
            ScheduledTask::SendDispatch(message_id)
        };

        let task_bn = self.block_height().saturating_add(delay);

        self.task_pool.add(task_bn, task).unwrap_or_else(|e| {
            let err_msg = format!(
                "send_delayed_dispatch: failed adding task for delayed message sending. \
                    Message to user - {to_user}, message id - {message_id}. Got error - {e:?}"
            );

            unreachable!("{err_msg}");
        });
    }

    pub(crate) fn send_user_message(
        &mut self,
        origin_msg: MessageId,
        message: Message,
        reservation: Option<ReservationId>,
    ) {
        let threshold = MAILBOX_THRESHOLD;

        let msg_id = reservation
            .map(Origin::into_origin)
            .unwrap_or_else(|| origin_msg.into_origin());

        let gas_limit = message
            .gas_limit()
            .or_else(|| {
                let gas_limit = self.gas_tree.get_limit(msg_id).unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message: failed getting message gas limit. \
                            Lock sponsor id - {msg_id}. Got error - {e:?}"
                    );

                    unreachable!("{err_msg}");
                });

                // If available gas is greater then threshold,
                // than threshold can be used.
                (gas_limit >= threshold).then_some(threshold)
            })
            .unwrap_or_default();

        let from = message.source();
        let to = message.destination();
        let value = message.value();

        let stored_message = message.into_stored();
        let message: UserMessage = stored_message
            .clone()
            .try_into()
            .expect("failed to convert stored message to user message");

        if Accounts::balance(from) != 0 {
            self.bank.deposit_value(from, value, false);
        }
        let _ = if message.details().is_none() && gas_limit >= threshold {
            let hold = HoldBoundBuilder::new(StorageType::Mailbox).maximum_for(self, gas_limit);

            if hold.expected_duration(self).is_zero() {
                let err_msg = format!(
                    "send_user_message: mailbox message got zero duration hold bound for storing. \
                    Gas limit - {gas_limit}, block cost - {cost}, source - {from:?}",
                    cost = Self::cost_by_storage_type(StorageType::Mailbox)
                );

                unreachable!("{err_msg}");
            }

            self.gas_tree
                .cut(msg_id, message.id(), gas_limit)
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message: failed creating cut node. \
                        Origin node - {msg_id}, cut node id - {id}, amount - {gas_limit}. \
                        Got error - {e:?}",
                        id = message.id()
                    );

                    unreachable!("{err_msg}");
                });

            self.gas_tree
                .lock(message.id(), LockId::Mailbox, gas_limit)
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message: failed locking gas for the user message mailbox. \
                        Message id - {message_id}, lock amount - {gas_limit}. Got error - {e:?}",
                        message_id = message.id(),
                    );

                    unreachable!("{err_msg}");
                });

            let message_id = message.id();
            let message: UserStoredMessage = message
                .clone()
                .try_into()
                .expect("failed to convert user message to user stored message");

            self.mailbox
                .insert(message, hold.expected())
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message: failed inserting message into mailbox. \
                        Message id - {message_id}, source - {from:?}, destination - {to:?}, \
                        expected bn - {bn:?}. Got error - {e:?}",
                        bn = hold.expected(),
                    );

                    unreachable!("{err_msg}");
                });

            self.task_pool
                .add(
                    hold.expected(),
                    ScheduledTask::RemoveFromMailbox(to, message_id),
                )
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message: failed adding task for removing from mailbox. \
                    Bn - {bn:?}, sent to - {to:?}, message id - {message_id}. \
                    Got error - {e:?}",
                        bn = hold.expected()
                    );

                    unreachable!("{err_msg}");
                });

            Some(hold.expected())
        } else {
            self.bank.transfer_value(from, to, value);

            if message.details().is_none() {
                // Creating auto reply message.
                let reply_message = ReplyMessage::auto(message.id());

                self.gas_tree
                    .split_with_value(true, origin_msg, reply_message.id(), 0)
                    .expect("GasTree corrupted");
                // Converting reply message into appropriate type for queueing.
                let reply_dispatch = reply_message.into_stored_dispatch(
                    message.destination(),
                    message.source(),
                    message.id(),
                );

                self.dispatches.push_back(reply_dispatch);
            }

            None
        };
        self.log.push(stored_message);

        if let Some(reservation_id) = reservation {
            self.remove_gas_reservation_with_task(message.source(), reservation_id);
        }
    }

    pub(crate) fn send_user_message_after_delay(&mut self, message: UserMessage, to_mailbox: bool) {
        let from = message.source();
        let to = message.destination();
        let value = message.value();

        let _ = if to_mailbox {
            let gas_limit = self.gas_tree.get_limit(message.id()).unwrap_or_else(|e| {
                let err_msg = format!(
                    "send_user_message_after_delay: failed getting message gas limit. \
                        Message id - {message_id}. Got error - {e:?}",
                    message_id = message.id()
                );

                unreachable!("{err_msg}");
            });

            let hold = HoldBoundBuilder::new(StorageType::Mailbox).maximum_for(self, gas_limit);

            if hold.expected_duration(self).is_zero() {
                let err_msg = format!(
                    "send_user_message_after_delay: mailbox message (after delay) got zero duration hold bound for storing. \
                    Gas limit - {gas_limit}, block cost - {cost}, source - {from:?}",
                    cost = Self::cost_by_storage_type(StorageType::Mailbox)
                );

                unreachable!("{err_msg}");
            }

            self.gas_tree.lock(message.id(), LockId::Mailbox, gas_limit)
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message_after_delay: failed locking gas for the user message mailbox. \
                        Message id - {message_id}, lock amount - {gas_limit}. Got error - {e:?}",
                        message_id = message.id(),
                    );

                    unreachable!("{err_msg}");
                });

            let message_id = message.id();
            let message: UserStoredMessage = message
                .clone()
                .try_into()
                .expect("failed to convert user message to user stored message");
            self.mailbox
                .insert(message, hold.expected())
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_user_message_after_delay: failed inserting message into mailbox. \
                        Message id - {message_id}, source - {from:?}, destination - {to:?}, \
                        expected bn - {bn:?}. Got error - {e:?}",
                        bn = hold.expected(),
                    );

                    unreachable!("{err_msg}");
                });

            // Adding removal request in task pool

            self.task_pool
                .add(
                    hold.expected(),
                    ScheduledTask::RemoveFromMailbox(to, message_id),
                )
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                    "send_user_message_after_delay: failed adding task for removing from mailbox. \
                    Bn - {bn:?}, sent to - {to:?}, message id - {message_id}. \
                    Got error - {e:?}",
                    bn = hold.expected()
                );

                    unreachable!("{err_msg}");
                });

            Some(hold.expected())
        } else {
            self.bank.transfer_value(from, to, value);

            // Message is never reply here, because delayed reply sending forbidden.
            if message.details().is_none() {
                // Creating reply message.
                let reply_message = ReplyMessage::auto(message.id());

                // `GasNode` was created on send already.

                // Converting reply message into appropriate type for queueing.
                let reply_dispatch = reply_message.into_stored_dispatch(
                    message.destination(),
                    message.source(),
                    message.id(),
                );

                // Queueing dispatch.
                self.dispatches.push_back(reply_dispatch);
            }

            self.consume_and_retrieve(message.id());
            None
        };

        self.log.push(message.into());
    }

    /// Check if the current block number should trigger new epoch and reset
    /// the provided random data.
    pub(crate) fn check_epoch(&mut self) {
        let block_height = self.block_height();
        if block_height % EPOCH_DURATION_IN_BLOCKS == 0 {
            let mut rng = StdRng::seed_from_u64(
                INITIAL_RANDOM_SEED + (block_height / EPOCH_DURATION_IN_BLOCKS) as u64,
            );
            let mut random = [0u8; 32];
            rng.fill_bytes(&mut random);

            self.random_data = (random.to_vec(), block_height + 1);
        }
    }

    #[track_caller]
    pub(crate) fn update_storage_pages(
        &mut self,
        program_id: &ProgramId,
        memory_pages: BTreeMap<GearPage, PageBuf>,
    ) {
        Actors::modify(*program_id, |actor| {
            let pages_data = actor
                .unwrap_or_else(|| panic!("Actor id {program_id:?} not found"))
                .get_pages_data_mut()
                .expect("No pages data found for program");

            for (page, buf) in memory_pages {
                pages_data.insert(page, buf);
            }
        });
    }

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

    #[track_caller]
    fn validate_dispatch(&mut self, dispatch: &Dispatch) {
        let source = dispatch.source();
        let destination = dispatch.destination();

        if Actors::is_program(source) {
            panic!("Sending messages allowed only from users id");
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

    /// Call non-void meta function from actor stored in manager.
    /// Warning! This is a static call that doesn't change actors pages data.
    pub(crate) fn read_state_bytes(
        &mut self,
        payload: Vec<u8>,
        program_id: &ProgramId,
    ) -> Result<Vec<u8>> {
        let executable_actor_data = Actors::modify(*program_id, |actor| {
            if let Some(actor) = actor {
                Ok(actor.get_executable_actor_data())
            } else {
                Err(TestError::ActorNotFound(*program_id))
            }
        })?;

        if let Some((data, code)) = executable_actor_data {
            core_processor::informational::execute_for_reply::<Ext<LazyPagesNative>, _>(
                String::from("state"),
                code,
                Some(data.allocations),
                Some((*program_id, Default::default())),
                payload,
                GAS_ALLOWANCE,
                self.blocks_manager.get(),
            )
            .map_err(TestError::ReadStateError)
        } else if let Some(mut program_mock) = Actors::modify(*program_id, |actor| {
            actor.expect("Checked before").take_mock()
        }) {
            program_mock
                .state()
                .map_err(|err| TestError::ReadStateError(err.into()))
        } else {
            Err(TestError::ActorIsNotExecutable(*program_id))
        }
    }

    pub(crate) fn read_state_bytes_using_wasm(
        &mut self,
        payload: Vec<u8>,
        program_id: &ProgramId,
        fn_name: &str,
        wasm: Vec<u8>,
        args: Option<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        let mapping_code = Code::try_new_mock_const_or_no_rules(
            wasm,
            true,
            TryNewCodeConfig::new_no_exports_check(),
        )
        .map_err(|_| TestError::Instrumentation)?;

        let mapping_code = InstrumentedCodeAndId::from(CodeAndId::new(mapping_code))
            .into_parts()
            .0;

        let mut mapping_code_payload = args.unwrap_or_default();
        mapping_code_payload.append(&mut self.read_state_bytes(payload, program_id)?);

        core_processor::informational::execute_for_reply::<Ext<LazyPagesNative>, _>(
            String::from(fn_name),
            mapping_code,
            None,
            None,
            mapping_code_payload,
            GAS_ALLOWANCE,
            self.blocks_manager.get(),
        )
        .map_err(TestError::ReadStateError)
    }

    pub(crate) fn mint_to(&mut self, id: &ProgramId, value: Value) {
        Accounts::increase(*id, value);
    }

    pub(crate) fn balance_of(&self, id: &ProgramId) -> Value {
        Accounts::balance(*id)
    }

    pub(crate) fn read_mailbox_message(
        &mut self,
        to: ProgramId,
        from_mid: MessageId,
    ) -> Result<UserStoredMessage, MailboxErrorImpl> {
        let (message, hold_interval) = self.mailbox.remove(to, from_mid)?;

        let expected = hold_interval.finish;

        let user_id = message.destination();
        let from = message.source();

        self.charge_for_hold(message.id(), hold_interval, StorageType::Mailbox);
        self.consume_and_retrieve(message.id());

        self.bank.transfer_value(from, user_id, message.value());

        let _ = self.task_pool.delete(
            expected,
            ScheduledTask::RemoveFromMailbox(user_id, message.id()),
        );

        Ok(message)
    }

    #[track_caller]
    pub(crate) fn override_balance(&mut self, &id: &ProgramId, balance: Value) {
        if Actors::is_user(id) && balance < crate::EXISTENTIAL_DEPOSIT {
            panic!(
                "An attempt to override balance with value ({}) less than existential deposit ({})",
                balance,
                crate::EXISTENTIAL_DEPOSIT
            );
        }
        Accounts::override_balance(id, balance);
    }

    #[track_caller]
    pub(crate) fn read_memory_pages(&self, program_id: &ProgramId) -> BTreeMap<GearPage, PageBuf> {
        Actors::access(*program_id, |actor| {
            let program = match actor.unwrap_or_else(|| panic!("Actor id {program_id:?} not found"))
            {
                TestActor::Initialized(program) => program,
                TestActor::Uninitialized(_, program) => program.as_ref().unwrap(),
                TestActor::Dormant => panic!("Actor {program_id} isn't dormant"),
            };

            match program {
                Program::Genuine(program) => program.pages_data.clone(),
                Program::Mock(_) => panic!("Can't read memory of mock program"),
            }
        })
    }

    #[track_caller]
    fn init_success(&mut self, program_id: ProgramId) {
        Actors::modify(program_id, |actor| {
            actor
                .unwrap_or_else(|| panic!("Actor id {program_id:?} not found"))
                .set_initialized()
        });
    }

    #[track_caller]
    fn init_failure(&mut self, program_id: ProgramId, origin: ProgramId) {
        Actors::modify(program_id, |actor| {
            let actor = actor.unwrap_or_else(|| panic!("Actor id {program_id:?} not found"));
            *actor = TestActor::Dormant
        });

        let value = Accounts::balance(program_id);
        if value != 0 {
            Accounts::transfer(program_id, origin, value, false);
        }
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
        let block_config = BlockConfig {
            block_info: self.blocks_manager.get(),
            performance_multiplier: gsys::Percent::new(100),
            forbidden_funcs: Default::default(),
            reserve_for: RESERVE_FOR,
            gas_multiplier: gsys::GasMultiplier::from_value_per_gas(VALUE_PER_GAS),
            costs: ProcessCosts {
                ext: ExtCosts {
                    syscalls: Default::default(),
                    rent: RentCosts {
                        waitlist: WAITLIST_COST.into(),
                        dispatch_stash: DISPATCH_HOLD_COST.into(),
                        reservation: RESERVATION_COST.into(),
                    },
                    mem_grow: Default::default(),
                    mem_grow_per_page: Default::default(),
                },
                lazy_pages: LazyPagesCosts {
                    host_func_read: HOST_FUNC_READ_COST.into(),
                    host_func_write: HOST_FUNC_WRITE_COST.into(),
                    host_func_write_after_read: HOST_FUNC_WRITE_AFTER_READ_COST.into(),
                    load_page_storage_data: LOAD_PAGE_STORAGE_DATA_COST.into(),
                    signal_read: SIGNAL_READ_COST.into(),
                    signal_write: SIGNAL_WRITE_COST.into(),
                    signal_write_after_read: SIGNAL_WRITE_AFTER_READ_COST.into(),
                },
                read: READ_COST.into(),
                read_per_byte: READ_PER_BYTE_COST.into(),
                write: WRITE_COST.into(),
                instrumentation: MODULE_INSTRUMENTATION_COST.into(),
                instrumentation_per_byte: MODULE_INSTRUMENTATION_BYTE_COST.into(),
                instantiation_costs: InstantiationCosts {
                    code_section_per_byte: MODULE_CODE_SECTION_INSTANTIATION_BYTE_COST.into(),
                    data_section_per_byte: MODULE_DATA_SECTION_INSTANTIATION_BYTE_COST.into(),
                    global_section_per_byte: MODULE_GLOBAL_SECTION_INSTANTIATION_BYTE_COST.into(),
                    table_section_per_byte: MODULE_TABLE_SECTION_INSTANTIATION_BYTE_COST.into(),
                    element_section_per_byte: MODULE_ELEMENT_SECTION_INSTANTIATION_BYTE_COST.into(),
                    type_section_per_byte: MODULE_TYPE_SECTION_INSTANTIATION_BYTE_COST.into(),
                },
                load_allocations_per_interval: LOAD_ALLOCATIONS_PER_INTERVAL.into(),
            },
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

    pub(crate) fn update_genuine_program<R, F: FnOnce(&mut GenuineProgram) -> R>(
        &mut self,
        id: ProgramId,
        op: F,
    ) -> Option<R> {
        Actors::modify(id, |actor| {
            actor.and_then(|actor| actor.genuine_program_mut().map(op))
        })
    }

    fn cost_by_storage_type(storage_type: StorageType) -> u64 {
        // Cost per block based on the storage used for holding
        match storage_type {
            StorageType::Code => todo!("#646"),
            StorageType::Waitlist => WAITLIST_COST,
            StorageType::Mailbox => MAILBOX_COST,
            StorageType::DispatchStash => DISPATCH_HOLD_COST,
            StorageType::Program => todo!("#646"),
            StorageType::Reservation => RESERVATION_COST,
        }
    }

    /// Spends given amount of gas from given `MessageId` in `GasTree`.
    ///
    /// Represents logic of burning gas by transferring gas from
    /// current `GasTree` owner to actual block producer.
    pub fn spend_gas(&mut self, id: MessageId, amount: u64) {
        if amount.is_zero() {
            return;
        }

        self.gas_tree.spend(id, amount).unwrap_or_else(|e| {
            let err_msg = format!(
                "spend_gas: failed spending gas. Message id - {id}, amount - {amount}. Got error - {e:?}"
            );

            unreachable!("{err_msg}");
        });

        let (external, multiplier, _) = self.gas_tree.get_origin_node(id).unwrap_or_else(|e| {
            let err_msg = format!(
                "spend_gas: failed getting origin node for the current one. Message id - {id}, Got error - {e:?}"
            );
            unreachable!("{err_msg}");
        });

        self.bank.spend_gas(external.cast(), amount, multiplier)
    }

    // todo [sab] separate this stuff

    fn wait_dipatch_impl(
        &self,
        dispatch: StoredDispatch,
        duration: Option<BlockNumber>,
        reason: MessageWaitedReason,
    ) {
        use MessageWaitedRuntimeReason::*;

        let hold_builder = HoldBoundBuilder::new(StorageType::Waitlist);

        let maximal_hold = hold_builder.maximum_for_message(self, dispatch.id());

        let hold = if let Some(duration) = duration {
            hold_builder.duration(self, duration).min(maximal_hold)
        } else {
            maximal_hold
        };

        let message_id = dispatch.id();
        let destination = dispatch.destination();

        if hold.expected_duration(self).is_zero() {
            let gas_limit = self.gas_tree.get_limit(dispatch.id()).unwrap_or_else(|e| {
                let err_msg = format!(
                    "wait_dispatch: failed getting message gas limit. Message id - {message_id}. \
                        Got error - {e:?}",
                    message_id = dispatch.id()
                );

                unreachable!("{err_msg}");
            });

            let err_msg = format!(
                "wait_dispatch: message got zero duration hold bound for waitlist. \
                Requested duration - {duration:?}, gas limit - {gas_limit}, \
                wait reason - {reason:?}, message id - {}.",
                dispatch.id(),
            );

            unreachable!("{err_msg}");
        }

        // Locking funds for holding.
        let lock_id = hold.lock_id().unwrap_or_else(|| {
            // Waitlist storage is guaranteed to have an associated lock id
            let err_msg = "wait_dispatch: No associated lock id for the waitlist storage";

            unreachable!("{err_msg}");
        });
        self.gas_tree
            .lock(message_id, lock_id, hold.lock_amount(self))
            .unwrap_or_else(|e| {
                let err_msg = format!(
                    "wait_dispatch: failed locking gas for the waitlist hold. \
                    Message id - {message_id}, lock amount - {lock}. Got error - {e:?}",
                    lock = hold.lock_amount(self)
                );

                unreachable!("{err_msg}");
            });

        match reason {
            MessageWaitedReason::Runtime(WaitForCalled | WaitUpToCalledFull) => {
                let expected = hold.expected();
                let task = ScheduledTask::WakeMessage(destination, message_id);

                if !self.task_pool.contains(&expected, &task) {
                    self.task_pool.add(expected, task).unwrap_or_else(|e| {
                        let err_msg = format!(
                            "wait_dispatch: failed adding task for waking message. \
                            Expected bn - {expected:?}, program id - {destination}, message id - {message_id}. Got error - {e:?}",
                        );

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}");
                    });
                }
            }
            MessageWaitedReason::Runtime(WaitCalled | WaitUpToCalled) => {
                self.task_pool.add(
                    hold.expected(),
                    ScheduledTask::RemoveFromWaitlist(dispatch.destination(), dispatch.id()),
                )
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "wait_dispatch: failed adding task for removing message from waitlist. \
                        Expected bn - {bn:?}, program id - {destination}, message id - {message_id}. Got error - {e:?}",
                        bn = hold.expected(),
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
            }
            MessageWaitedReason::System(reason) => match reason {},
        }

        self.waitlist.insert(dispatch, hold.expected())
            .unwrap_or_else(|e| {
                let err_msg = format!(
                    "wait_dispatch: failed inserting message to the wailist. \
                    Expected bn - {bn:?}, program id - {destination}, message id - {message_id}. Got error - {e:?}",
                    bn = hold.expected(),
                );

                unreachable!("{err_msg}");
            });
    }

    fn wake_dispatch_impl(
        &mut self,
        program_id: ProgramId,
        message_id: MessageId,
    ) -> Result<StoredDispatch, WaitlistErrorImpl> {
        let (waitlisted, hold_interval) = self.waitlist.remove(program_id, message_id)?;
        let expected_bn = hold_interval.finish;

        self.charge_for_hold(waitlisted.id(), hold_interval, StorageType::Waitlist);

        let _ = self.task_pool.delete(
            expected_bn,
            ScheduledTask::RemoveFromWaitlist(waitlisted.destination(), waitlisted.id()),
        );

        Ok(waitlisted)
    }
}
