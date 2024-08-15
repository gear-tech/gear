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

mod journal;
mod task;

use crate::{
    blocks::BlocksManager,
    gas_tree::GasTreeManager,
    log::{BlockRunResult, CoreLog},
    mailbox::MailboxManager,
    program::{Gas, WasmProgram},
    task_pool::TaskPoolManager,
    Result, TestError, DISPATCH_HOLD_COST, EPOCH_DURATION_IN_BLOCKS, EXISTENTIAL_DEPOSIT,
    GAS_ALLOWANCE, HOST_FUNC_READ_COST, HOST_FUNC_WRITE_AFTER_READ_COST, HOST_FUNC_WRITE_COST,
    INITIAL_RANDOM_SEED, LOAD_ALLOCATIONS_PER_INTERVAL, LOAD_PAGE_STORAGE_DATA_COST,
    MAILBOX_THRESHOLD, MAX_RESERVATIONS, MODULE_CODE_SECTION_INSTANTIATION_BYTE_COST,
    MODULE_DATA_SECTION_INSTANTIATION_BYTE_COST, MODULE_ELEMENT_SECTION_INSTANTIATION_BYTE_COST,
    MODULE_GLOBAL_SECTION_INSTANTIATION_BYTE_COST, MODULE_INSTRUMENTATION_BYTE_COST,
    MODULE_INSTRUMENTATION_COST, MODULE_TABLE_SECTION_INSTANTIATION_BYTE_COST,
    MODULE_TYPE_SECTION_INSTANTIATION_BYTE_COST, READ_COST, READ_PER_BYTE_COST, RESERVATION_COST,
    RESERVE_FOR, SIGNAL_READ_COST, SIGNAL_WRITE_AFTER_READ_COST, SIGNAL_WRITE_COST, VALUE_PER_GAS,
    WAITLIST_COST, WRITE_COST,
};
use core_processor::{
    common::*,
    configs::{
        BlockConfig, ExtCosts, InstantiationCosts, ProcessCosts, RentCosts, TESTS_MAX_PAGES_NUMBER,
    },
    ContextChargedForCode, ContextChargedForInstrumentation, Ext,
};
use gear_common::{
    auxiliary::{gas_provider::PlainNodeId, mailbox::MailboxErrorImpl, BlockNumber},
    gas_provider::Imbalance,
    scheduler::{ScheduledTask, StorageType},
    storage::Interval,
    LockId, Origin,
};
use gear_core::{
    code::{Code, CodeAndId, InstrumentedCode, InstrumentedCodeAndId, TryNewCodeConfig},
    ids::{prelude::*, CodeId, MessageId, ProgramId, ReservationId},
    memory::PageBuf,
    message::{
        Dispatch, DispatchKind, ReplyMessage, ReplyPacket, StoredDelayedDispatch, StoredDispatch,
        StoredMessage,
    },
    pages::{num_traits::Zero, numerated::tree::IntervalsTree, GearPage, WasmPage},
    reservation::{GasReservationMap, GasReservationSlot},
};
use gear_core_errors::{ErrorReplyReason, SimpleExecutionError};
use gear_lazy_pages_common::LazyPagesCosts;
use gear_lazy_pages_native_interface::LazyPagesNative;
use gear_wasm_instrument::gas_metering::Schedule;
use gsys::BlockNumberWithHash;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use std::{
    cell::{Ref, RefCell, RefMut},
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    convert::TryInto,
    fmt::Debug,
    mem,
    rc::Rc,
};

const OUTGOING_LIMIT: u32 = 1024;
const OUTGOING_BYTES_LIMIT: u32 = 64 * 1024 * 1024;

pub(crate) type Balance = u128;

#[derive(Debug)]
pub(crate) enum TestActor {
    Initialized(Program),
    // Contract: program is always `Some`, option is used to take ownership
    Uninitialized(Option<MessageId>, Option<Program>),
    Dormant,
    User,
}

impl TestActor {
    fn new(init_message_id: Option<MessageId>, program: Program) -> Self {
        TestActor::Uninitialized(init_message_id, Some(program))
    }

    // # Panics
    // If actor is initialized or dormant
    #[track_caller]
    fn set_initialized(&mut self) {
        assert!(
            self.is_uninitialized(),
            "can't transmute actor, which isn't uninitialized"
        );

        if let TestActor::Uninitialized(_, maybe_prog) = self {
            *self = TestActor::Initialized(
                maybe_prog
                    .take()
                    .expect("actor storage contains only `Some` values by contract"),
            );
        }
    }

    fn is_dormant(&self) -> bool {
        matches!(self, TestActor::Dormant)
    }

    fn is_uninitialized(&self) -> bool {
        matches!(self, TestActor::Uninitialized(..))
    }

    pub(crate) fn genuine_program(&self) -> Option<&GenuineProgram> {
        match self {
            TestActor::Initialized(Program::Genuine(program))
            | TestActor::Uninitialized(_, Some(Program::Genuine(program))) => Some(program),
            _ => None,
        }
    }

    fn genuine_program_mut(&mut self) -> Option<&mut GenuineProgram> {
        match self {
            TestActor::Initialized(Program::Genuine(program))
            | TestActor::Uninitialized(_, Some(Program::Genuine(program))) => Some(program),
            _ => None,
        }
    }

    pub fn get_pages_data(&self) -> Option<&BTreeMap<GearPage, PageBuf>> {
        self.genuine_program().map(|program| &program.pages_data)
    }

    fn get_pages_data_mut(&mut self) -> Option<&mut BTreeMap<GearPage, PageBuf>> {
        self.genuine_program_mut()
            .map(|program| &mut program.pages_data)
    }

    // Takes ownership over mock program, putting `None` value instead of it.
    fn take_mock(&mut self) -> Option<Box<dyn WasmProgram>> {
        match self {
            TestActor::Initialized(Program::Mock(mock))
            | TestActor::Uninitialized(_, Some(Program::Mock(mock))) => mock.take(),
            _ => None,
        }
    }

    // Gets a new executable actor derived from the inner program.
    fn get_executable_actor_data(&self) -> Option<(ExecutableActorData, InstrumentedCode)> {
        self.genuine_program().map(|program| {
            (
                ExecutableActorData {
                    allocations: program.allocations.clone(),
                    code_id: program.code_id,
                    code_exports: program.code.exports().clone(),
                    static_pages: program.code.static_pages(),
                    gas_reservation_map: program.gas_reservation_map.clone(),
                    memory_infix: Default::default(),
                },
                program.code.clone(),
            )
        })
    }
}

#[derive(Debug)]
pub(crate) struct GenuineProgram {
    pub code_id: CodeId,
    pub code: InstrumentedCode,
    pub allocations: IntervalsTree<WasmPage>,
    pub pages_data: BTreeMap<GearPage, PageBuf>,
    pub gas_reservation_map: GasReservationMap,
}

#[derive(Debug)]
pub(crate) enum Program {
    Genuine(GenuineProgram),
    // Contract: is always `Some`, option is used to take ownership
    Mock(Option<Box<dyn WasmProgram>>),
}

impl Program {
    pub(crate) fn new_mock(mock: impl WasmProgram + 'static) -> Self {
        Program::Mock(Some(Box::new(mock)))
    }
}

#[derive(Default, Debug, Clone)]
pub(crate) struct Actors(Rc<RefCell<BTreeMap<ProgramId, (TestActor, Balance)>>>);

impl Actors {
    pub fn borrow(&self) -> Ref<'_, BTreeMap<ProgramId, (TestActor, Balance)>> {
        self.0.borrow()
    }

    pub fn borrow_mut(&mut self) -> RefMut<'_, BTreeMap<ProgramId, (TestActor, Balance)>> {
        self.0.borrow_mut()
    }

    fn insert(
        &mut self,
        program_id: ProgramId,
        actor_and_balance: (TestActor, Balance),
    ) -> Option<(TestActor, Balance)> {
        self.0.borrow_mut().insert(program_id, actor_and_balance)
    }

    pub fn contains_key(&self, program_id: &ProgramId) -> bool {
        self.0.borrow().contains_key(program_id)
    }

    fn remove(&mut self, program_id: &ProgramId) -> Option<(TestActor, Balance)> {
        self.0.borrow_mut().remove(program_id)
    }
}

/// Simple boolean for whether an account needs to be kept in existence.
#[derive(PartialEq)]
pub(crate) enum MintMode {
    /// Operation must not result in the account going out of existence.
    KeepAlive,
    /// Operation may result in account going out of existence.
    AllowDeath,
}

#[derive(Debug, Default)]
pub(crate) struct ExtManager {
    // State metadata
    pub(crate) blocks_manager: BlocksManager,
    pub(crate) random_data: (Vec<u8>, u32),

    // Messaging and programs meta
    pub(crate) msg_nonce: u64,
    pub(crate) id_nonce: u64,

    // State
    pub(crate) actors: Actors,
    pub(crate) opt_binaries: BTreeMap<CodeId, Vec<u8>>,
    pub(crate) meta_binaries: BTreeMap<CodeId, Vec<u8>>,
    pub(crate) dispatches: VecDeque<StoredDispatch>,
    pub(crate) mailbox: MailboxManager,
    pub(crate) task_pool: TaskPoolManager,
    pub(crate) wait_list: BTreeMap<(ProgramId, MessageId), (StoredDispatch, Option<BlockNumber>)>,
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

    // Weights
    // Store here to not always construct Schedule
    pub(crate) weights: Weights,
}

#[derive(Default)]
struct Weights {
    schedule: Schedule,
}

impl std::fmt::Debug for Weights {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Weights")
    }
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

    pub(crate) fn store_new_actor(
        &mut self,
        program_id: ProgramId,
        program: Program,
        init_message_id: Option<MessageId>,
    ) -> Option<(TestActor, Balance)> {
        if let Program::Genuine(GenuineProgram { code, .. }) = &program {
            self.store_new_code(code.code().to_vec());
        }
        self.actors
            .insert(program_id, (TestActor::new(init_message_id, program), 0))
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
        while self.actors.contains_key(&self.id_nonce.into()) {
            self.id_nonce += 1;
        }
        self.id_nonce
    }

    pub(crate) fn remove_gas_reservation_impl(
        &mut self,
        program_id: ProgramId,
        reservation: ReservationId,
    ) -> GasReservationSlot {
        let slot = self.update_genuine_program(program_id, |p| {
            p.gas_reservation_map
                .remove(&reservation)
                .unwrap_or_else(|| {
                    let err_msg = format!("ExtManager::remove_gas_reservation_impl: failed removing gas reservation. \
                    Reservation {reservation} doesn't exist.");

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}")
                })
        }).unwrap_or_else(|| {
            unreachable!("failed to update program {program_id}")
        });

        self.remove_gas_reservation_slot(reservation, slot)
    }

    pub(crate) fn remove_gas_reservation_with_task(
        &mut self,
        program_id: ProgramId,
        reservation: ReservationId,
    ) {
        let slot = self.remove_gas_reservation_impl(program_id, reservation);

        let _ = self.task_pool.delete(
            slot.finish,
            ScheduledTask::RemoveGasReservation(program_id, reservation),
        );
    }

    pub(crate) fn remove_gas_reservation_slot(
        &mut self,
        reservation: ReservationId,
        slot: GasReservationSlot,
    ) -> GasReservationSlot {
        let interval = Interval {
            start: slot.start,
            finish: slot.finish,
        };

        self.charge_for_hold(reservation, interval, StorageType::Reservation);
        self.consume_and_retrieve(reservation);

        slot
    }

    pub(crate) fn consume_and_retrieve(&mut self, id: impl Origin) {
        let outcome = self.gas_tree.consume(id).unwrap_or_else(|e| {
            let err_msg = format!(
                "consume_and_retrieve: failed consuming the rest of gas. Got error - {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });

        if let Some((imbalance, multiplier, exteernal)) = outcome {
            let gas_left = imbalance.peek();

            if !gas_left.is_zero() {
                // TODO: withdraw from bank
            }
        }
    }

    /// Insert message into the delayed queue.
    fn send_delayed_dispatch(
        &mut self,
        origin_msg: MessageId,
        dispatch: Dispatch,
        delay: u32,
        to_user: bool,
        reservation: Option<ReservationId>,
    ) {
        if delay.is_zero() {
            let err_msg = "send_delayed_dispatch: delayed sending with zero delay appeared";

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        }

        let message_id = dispatch.id();

        if self.dispatches_stash.contains_key(&message_id) {
            let err_msg = format!(
                "send_delayed_dispatch: stash already has the message id - {id}",
                id = dispatch.id()
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        }

        // Validating dispatch wasn't sent from system with delay.
        if dispatch.is_error_reply() || matches!(dispatch.kind(), DispatchKind::Signal) {
            let err_msg = format!(
                "send_delayed_dispatch: message of an invalid kind is sent: {kind:?}",
                kind = dispatch.kind()
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        }

        let mut to_mailbox = false;

        let sender_node = reservation
            .map(|id| id.cast::<PlainNodeId>())
            .unwrap_or_else(|| origin_msg.cast::<PlainNodeId>())
            .cast::<MessageId>();

        let from = dispatch.source();
        let _value = dispatch.value().try_into().unwrap_or(u64::MAX);

        let hold_builder = HoldBoundBuilder::new(StorageType::DispatchStash);

        let delay_hold = hold_builder.duration(self, delay.try_into().unwrap_or(u32::MAX));
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

                        log::error!("{err_msg}");
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

                    log::error!("{err_msg}");
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

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

            self.gas_tree.lock(dispatch.id(), lock_id, delay_hold.lock_amount(self))
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "send_delayed_dispatch: failed locking gas for the user message stash hold. \
                        Message id - {message_id}, lock amount - {lock}. Got error - {e:?}",
                        message_id = dispatch.id(),
                        lock = delay_hold.lock_amount(self));
                        log::error!("{err_msg}");
                        unreachable!("{err_msg}");
                });

            if delay_hold.expected_duration(self).is_zero() {
                let err_msg = format!(
                    "send_delayed_dispatch: user message got zero duration hold bound for dispatch stash. \
                    Requested duration - {delay}, block cost - {cost}, source - {from:?}",
                    cost = Self::cost_by_storage_type(StorageType::DispatchStash)
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            }

            delay_hold.expected()
        } else {
            match (dispatch.gas_limit(), reservation) {
                (Some(gas_limit), None) => self.split_with_value(
                    sender_node,
                    dispatch.id(),
                    gas_limit.saturating_add(gas_for_delay),
                    dispatch.is_reply(),
                ),

                (None, None) => self.split(sender_node, dispatch.id(), dispatch.is_reply()),
                (Some(gas_limit), Some(reservation_id)) => {
                    let err_msg = format!(
                        "send_delayed_dispatch: sending dispatch with gas from reservation isn't implemented. \
                        Message - {message_id}, sender - {sender}, gas limit - {gas_limit}, reservation - {reservation_id}",
                        message_id = dispatch.id(),
                        sender = dispatch.source(),
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                }

                (None, Some(reservation_id)) => {
                    self.split(reservation_id.cast(), dispatch.id(), dispatch.is_reply());
                    self.remove_gas_reservation_with_task(dispatch.source(), reservation_id);
                }
            }

            let lock_id = delay_hold.lock_id().unwrap_or_else(|| {
                // Dispatch stash storage is guaranteed to have an associated lock id
                let err_msg =
                    "send_delayed_dispatch: No associated lock id for the dispatch stash storage";

                log::error!("{err_msg}");
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

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });

            if delay_hold.expected_duration(self).is_zero() {
                let err_msg = format!(
                    "send_delayed_dispatch: program message got zero duration hold bound for dispatch stash. \
                    Requested duration - {delay}, block cost - {cost}, source - {from:?}",
                    cost = Self::cost_by_storage_type(StorageType::DispatchStash)
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            }

            delay_hold.expected()
        };

        if !dispatch.value().is_zero() {
            // TODO: gear bank deposit
        }

        let messaeg_id = dispatch.id();

        let start_bn = self.blocks_manager.get().height;
        let delay_interval = Interval {
            start: start_bn,
            finish: interval_finish,
        };

        self.dispatches_stash
            .insert(messaeg_id, (dispatch.into_stored_delayed(), delay_interval));

        let task = if to_user {
            ScheduledTask::SendUserMessage {
                message_id,
                to_mailbox,
            }
        } else {
            ScheduledTask::SendDispatch(message_id)
        };

        let task_bn = self.blocks_manager.get().height.saturating_add(delay);

        self.task_pool.add(task_bn, task).unwrap_or_else(|e| {
            let err_msg = format!(
                "send_delayed_dispatch: failed adding task for delayed message sending. \
                    Message to user - {to_user}, message id - {message_id}. Got error - {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });
    }

    fn split_with_value(
        &mut self,
        key: MessageId,
        new_key: MessageId,
        amount: u64,
        is_reply: bool,
    ) {
        if !is_reply || !self.gas_tree.exists_and_deposit(new_key.clone()) {
            self.gas_tree.split_with_value(is_reply, key.clone(), new_key.clone(), amount)
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "split_with_value: failed to split with value gas node. Original message id - {key}, \
                        new message id - {new_key}. amount - {amount}, is_reply - {is_reply}. \
                        Got error - {e:?}",
                        key = key,
                        new_key = new_key,
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
        }
    }

    fn split(&mut self, key: MessageId, new_key: MessageId, is_reply: bool) {
        if !is_reply || !self.gas_tree.exists_and_deposit(new_key.clone()) {
            self.gas_tree.split(is_reply, key.clone(), new_key.clone())
                .unwrap_or_else(|e| {
                    let err_msg = format!(
                        "split_with_value: failed to split with value gas node. Original message id - {key}, \
                        new message id - {new_key}. is_reply - {is_reply}. \
                        Got error - {e:?}",
                        key = key,
                        new_key = new_key,
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
        }
    }

    /// Check if the current block number should trigger new epoch and reset
    /// the provided random data.
    pub(crate) fn check_epoch(&mut self) {
        let block_height = self.blocks_manager.get().height;
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
        let mut actors = self.actors.borrow_mut();
        let program = &mut actors
            .get_mut(program_id)
            .unwrap_or_else(|| panic!("Actor {program_id} not found"))
            .0;

        let pages_data = program
            .get_pages_data_mut()
            .expect("No pages data found for program");

        for (page, buf) in memory_pages {
            pages_data.insert(page, buf);
        }
    }

    pub(crate) fn validate_and_route_dispatch(&mut self, dispatch: Dispatch) -> MessageId {
        self.validate_dispatch(&dispatch);
        let gas_limit = dispatch
            .gas_limit()
            .unwrap_or_else(|| unreachable!("message from program API always has gas"));
        self.gas_tree
            .create(dispatch.source(), dispatch.id(), gas_limit)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
        self.route_dispatch(dispatch)
    }

    pub(crate) fn route_dispatch(&mut self, dispatch: Dispatch) -> MessageId {
        let stored_dispatch = dispatch.into_stored();
        if self.is_user(&stored_dispatch.destination()) {
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
        let new_block_bn = self.blocks_manager.get().height;

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

            let mut actors = self.actors.borrow_mut();
            let (actor, balance) = actors
                .get_mut(&dispatch.destination())
                .expect("Somehow message queue contains message for user");
            let balance = *balance;

            if actor.is_dormant() {
                drop(actors);
                self.process_dormant(balance, dispatch);
            } else if let Some((data, code)) = actor.get_executable_actor_data() {
                drop(actors);
                self.process_normal(balance, data, code, dispatch);
            } else if let Some(mock) = actor.take_mock() {
                drop(actors);
                self.process_mock(mock, dispatch);
            } else {
                unreachable!();
            }

            total_processed += 1;
        }

        total_processed
    }

    #[track_caller]
    fn validate_dispatch(&mut self, dispatch: &Dispatch) {
        if self.is_program(&dispatch.source()) {
            panic!("Sending messages allowed only from users id");
        }

        let mut actors = self.actors.borrow_mut();
        let (_, balance) = actors
            .entry(dispatch.source())
            .or_insert((TestActor::User, 0));

        if *balance < dispatch.value() {
            panic!(
                "Insufficient value: user ({}) tries to send \
                ({}) value, while his balance ({})",
                dispatch.source(),
                dispatch.value(),
                balance
            );
        } else {
            *balance -= dispatch.value();
            if *balance < crate::EXISTENTIAL_DEPOSIT {
                *balance = 0;
            }
        }
    }

    /// Call non-void meta function from actor stored in manager.
    /// Warning! This is a static call that doesn't change actors pages data.
    pub(crate) fn read_state_bytes(
        &mut self,
        payload: Vec<u8>,
        program_id: &ProgramId,
    ) -> Result<Vec<u8>> {
        let mut actors = self.actors.borrow_mut();
        let (actor, _balance) = actors
            .get_mut(program_id)
            .ok_or_else(|| TestError::ActorNotFound(*program_id))?;

        if let Some((data, code)) = actor.get_executable_actor_data() {
            drop(actors);

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
        } else if let Some(mut program_mock) = actor.take_mock() {
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

    pub(crate) fn is_user(&self, id: &ProgramId) -> bool {
        matches!(
            self.actors.borrow().get(id),
            Some((TestActor::User, _)) | None
        )
    }

    pub(crate) fn is_active_program(&self, id: &ProgramId) -> bool {
        matches!(
            self.actors.borrow().get(id),
            Some((TestActor::Initialized(_), _)) | Some((TestActor::Uninitialized(_, _), _))
        )
    }

    pub(crate) fn is_program(&self, id: &ProgramId) -> bool {
        matches!(
            self.actors.borrow().get(id),
            Some((TestActor::Initialized(_), _))
                | Some((TestActor::Uninitialized(_, _), _))
                | Some((TestActor::Dormant, _))
        )
    }

    pub(crate) fn mint_to(&mut self, id: &ProgramId, value: Balance, mint_mode: MintMode) {
        if mint_mode == MintMode::KeepAlive && value < crate::EXISTENTIAL_DEPOSIT {
            panic!(
                "An attempt to mint value ({}) less than existential deposit ({})",
                value,
                crate::EXISTENTIAL_DEPOSIT
            );
        }

        let mut actors = self.actors.borrow_mut();
        let (_, balance) = actors.entry(*id).or_insert((TestActor::User, 0));
        *balance = balance.saturating_add(value);
    }

    pub(crate) fn balance_of(&self, id: &ProgramId) -> Balance {
        self.actors
            .borrow()
            .get(id)
            .map(|(_, balance)| *balance)
            .unwrap_or_default()
    }

    pub(crate) fn claim_value_from_mailbox(
        &mut self,
        to: ProgramId,
        from_mid: MessageId,
    ) -> Result<(), MailboxErrorImpl> {
        let (message, _) = self.mailbox.remove(to, from_mid)?;

        self.send_value(
            message.source(),
            Some(message.destination()),
            message.value(),
        );
        self.message_consumed(message.id());

        Ok(())
    }

    #[track_caller]
    pub(crate) fn override_balance(&mut self, id: &ProgramId, balance: Balance) {
        if self.is_user(id) && balance < crate::EXISTENTIAL_DEPOSIT {
            panic!(
                "An attempt to override balance with value ({}) less than existential deposit ({})",
                balance,
                crate::EXISTENTIAL_DEPOSIT
            );
        }

        let mut actors = self.actors.borrow_mut();
        let (_, actor_balance) = actors.entry(*id).or_insert((TestActor::User, 0));
        *actor_balance = balance;
    }

    #[track_caller]
    pub(crate) fn read_memory_pages(&self, program_id: &ProgramId) -> BTreeMap<GearPage, PageBuf> {
        let actors = self.actors.borrow();
        let program = &actors
            .get(program_id)
            .unwrap_or_else(|| panic!("Actor {program_id} not found"))
            .0;

        let program = match program {
            TestActor::Initialized(program) => program,
            TestActor::Uninitialized(_, program) => program.as_ref().unwrap(),
            TestActor::Dormant | TestActor::User => panic!("Actor {program_id} isn't a program"),
        };

        match program {
            Program::Genuine(program) => program.pages_data.clone(),
            Program::Mock(_) => panic!("Can't read memory of mock program"),
        }
    }

    #[track_caller]
    fn init_success(&mut self, program_id: ProgramId) {
        let mut actors = self.actors.borrow_mut();
        let (actor, _) = actors
            .get_mut(&program_id)
            .expect("Can't find existing program");

        actor.set_initialized();
    }

    #[track_caller]
    fn init_failure(&mut self, program_id: ProgramId) {
        let mut actors = self.actors.borrow_mut();
        let (actor, _) = actors
            .get_mut(&program_id)
            .expect("Can't find existing program");

        *actor = TestActor::Dormant;
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
        self.actors
            .borrow_mut()
            .entry(program_id)
            .and_modify(|(actor, _)| {
                if let TestActor::Initialized(old_mock) = actor {
                    *old_mock = Program::Mock(Some(mock));
                }
            });
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

    fn remove_reservation(&mut self, id: ProgramId, reservation: ReservationId) -> Option<bool> {
        let was_in_map = self.update_genuine_program(id, |genuine_program| {
            genuine_program
                .gas_reservation_map
                .remove(&reservation)
                .is_some()
        })?;

        if was_in_map {
            self.gas_tree
                .consume(reservation)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));
        } else {
            log::error!("Tried to remove unexistent reservation {reservation} for program {id}.");
        }

        Some(was_in_map)
    }

    pub(crate) fn update_genuine_program<R, F: FnOnce(&mut GenuineProgram) -> R>(
        &mut self,
        id: ProgramId,
        op: F,
    ) -> Option<R> {
        let mut actors = self.actors.borrow_mut();
        actors
            .get_mut(&id)
            .and_then(|(actor, _)| actor.genuine_program_mut().map(op))
    }

    fn cost_by_storage_type(storage_type: StorageType) -> u64 {
        // Cost per block based on the storage used for holding
        let cost = match storage_type {
            StorageType::Code => todo!("#646"),
            StorageType::Waitlist => WAITLIST_COST,
            StorageType::Mailbox => MAILBOX_THRESHOLD,
            StorageType::DispatchStash => DISPATCH_HOLD_COST,
            StorageType::Program => todo!("#646"),
            StorageType::Reservation => RESERVATION_COST,
        };
        cost
    }

    pub(crate) fn charge_for_hold(
        &mut self,
        id: impl Origin,
        hold_interval: Interval<BlockNumber>,
        storage_type: StorageType,
    ) {
        let current = self.blocks_manager.get().height;

        // Deadline of the task.
        let deadline = hold_interval.finish.saturating_add(RESERVE_FOR);

        // The block number, which was the last paid for hold.
        //
        // Outdated tasks can end up being store for free - this case has to be
        // controlled by a correct selection of the `ReserveFor` constant.
        let paid_until = current.min(deadline);

        // holding duration
        let duration: u64 = paid_until
            .saturating_sub(hold_interval.start)
            .try_into()
            .unwrap_or(u64::MAX);

        // Cost per block based on the storage used for holding
        let cost = match storage_type {
            StorageType::Code => todo!("#646"),
            StorageType::Waitlist => WAITLIST_COST,
            StorageType::Mailbox => MAILBOX_THRESHOLD,
            StorageType::DispatchStash => DISPATCH_HOLD_COST,
            StorageType::Program => todo!("#646"),
            StorageType::Reservation => RESERVATION_COST,
        };

        let amount = storage_type.try_into().map_or_else(
            |_| duration.saturating_mul(cost),
            |lock_id| {
                let prepaid = self.gas_tree.unlock_all(id, lock_id).unwrap_or_else(|e| {
                    let err_msg = format!(
                        "charge_for_hold: failed unlocking locked gas.
                        Got error - {e:?}"
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });

                prepaid.min(duration.saturating_mul(cost))
            },
        );

        if !amount.is_zero() {
            // TODO
        }
    }
}

/// Hold bound, specifying cost of storage, expected block number for task to
/// create on it, deadlines and durations of holding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HoldBound {
    cost: u64,
    expected: BlockNumber,
    lock_id: Option<LockId>,
}

impl HoldBound {
    pub fn cost(&self) -> u64 {
        self.cost
    }

    pub fn expected(&self) -> BlockNumber {
        self.expected
    }

    pub fn lock_id(&self) -> Option<LockId> {
        self.lock_id
    }

    pub fn expected_duration(&self, manager: &ExtManager) -> BlockNumber {
        self.expected
            .saturating_sub(manager.blocks_manager.get().height)
    }

    pub fn deadline(&self) -> BlockNumber {
        self.expected.saturating_add(RESERVE_FOR)
    }

    pub fn deadline_duration(&self, manager: &ExtManager) -> BlockNumber {
        self.deadline()
            .saturating_sub(manager.blocks_manager.get().height)
    }

    pub fn lock_amount(&self, manager: &ExtManager) -> u64 {
        self.deadline_duration(manager)
            .try_into()
            .unwrap_or(u64::MAX)
            .saturating_mul(self.cost())
    }
}

pub struct HoldBoundBuilder {
    storage_type: StorageType,
    cost: u64,
}

impl HoldBoundBuilder {
    pub fn new(storage_type: StorageType) -> Self {
        Self {
            storage_type,
            cost: ExtManager::cost_by_storage_type(storage_type),
        }
    }

    pub fn at(self, expected: BlockNumber) -> HoldBound {
        HoldBound {
            cost: self.cost,
            expected,
            lock_id: self.storage_type.try_into().ok(),
        }
    }

    pub fn deadline(self, deadline: BlockNumber) -> HoldBound {
        let expected = deadline.saturating_sub(RESERVE_FOR);

        self.at(expected)
    }

    pub fn duration(self, manager: &ExtManager, duration: BlockNumber) -> HoldBound {
        let expected = manager.blocks_manager.get().height.saturating_add(duration);

        self.at(expected)
    }
}
