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

mod exec;
mod expend;
mod hold_bound;
mod journal;
mod memory;
mod reservations;
mod send_dispatch;
mod task;
mod wait_wake;

use crate::{
    constants::Value,
    log::{BlockRunResult, CoreLog},
    program::{Gas, WasmProgram},
    state::{
        accounts::Accounts,
        actors::{Actors, GenuineProgram, Program, TestActor},
        bank::Bank,
        blocks::BlocksManager,
        gas_tree::GasTreeManager,
        mailbox::MailboxManager,
        task_pool::TaskPoolManager,
        waitlist::WaitlistManager,
    },
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

    pub(crate) fn mint_to(&mut self, id: &ProgramId, value: Value) {
        Accounts::increase(*id, value);
    }

    pub(crate) fn balance_of(&self, id: &ProgramId) -> Value {
        Accounts::balance(*id)
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

    pub(crate) fn update_genuine_program<R, F: FnOnce(&mut GenuineProgram) -> R>(
        &mut self,
        id: ProgramId,
        op: F,
    ) -> Option<R> {
        Actors::modify(id, |actor| {
            actor.and_then(|actor| actor.genuine_program_mut().map(op))
        })
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

    pub(crate) fn send_reply_impl(
        &mut self,
        origin: ProgramId,
        reply_to_id: MessageId,
        raw_payload: impl AsRef<[u8]>,
        value: Value,
    ) -> Result<MessageId, MailboxErrorImpl> {
        let payload = raw_payload
            .as_ref()
            .to_vec()
            .try_into()
            .unwrap_or_else(|err| unreachable!("Can't send reply with such payload: {err:?}"));

        let mailboxed = self.read_mailbox_message(origin, reply_to_id)?;

        let destination = mailboxed.source();

        if !Actors::is_active_program(destination) {
            unreachable!("Can't send reply to a non-active program {destination:?}");
        }

        let reply_id = MessageId::generate_reply(mailboxed.id());

        // Set zero gas limit if reply deposit exists.
        let gas_limit = if self.gas_tree.exists_and_deposit(reply_id) {
            0
        } else {
            GAS_ALLOWANCE
        };

        self.bank.deposit_value(origin, value, false);
        self.bank.deposit_gas(origin, gas_limit, false);

        self.gas_tree
            .create(origin, reply_id, gas_limit, true)
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

        let message = ReplyMessage::from_packet(
            reply_id,
            ReplyPacket::new_with_gas(payload, gas_limit, value),
        );

        let dispatch = message.into_stored_dispatch(origin, destination, mailboxed.id());

        self.dispatches.push_back(dispatch);

        Ok(reply_id)
    }

    pub(crate) fn claim_value_impl(
        &mut self,
        origin: ProgramId,
        message_id: MessageId,
    ) -> Result<(), MailboxErrorImpl> {
        let mailboxed = self.read_mailbox_message(origin, message_id)?;

        if Actors::is_active_program(mailboxed.source()) {
            let message = ReplyMessage::auto(mailboxed.id());

            self.gas_tree
                .create(origin, message.id(), 0, true)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            let dispatch = message.into_stored_dispatch(origin, mailboxed.source(), mailboxed.id());

            self.dispatches.push_back(dispatch);
        }

        Ok(())
    }
}
