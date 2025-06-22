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

use crate::{
    constants::{Gas, Value},
    error::usage_panic,
    log::{BlockRunResult, CoreLog},
    state::{
        accounts::Accounts,
        actors::{Actors, Program, TestActor},
        bank::Bank,
        blocks::BlocksManager,
        gas_tree::GasTreeManager,
        mailbox::MailboxManager,
        task_pool::TaskPoolManager,
        waitlist::WaitlistManager,
    },
    Result, TestError, EPOCH_DURATION_IN_BLOCKS, EXISTENTIAL_DEPOSIT, GAS_ALLOWANCE,
    GAS_MULTIPLIER, INITIAL_RANDOM_SEED, MAX_RESERVATIONS, MAX_USER_GAS_LIMIT, RESERVE_FOR,
    VALUE_PER_GAS,
};
use core_processor::{
    common::*, configs::BlockConfig, ContextChargedForInstrumentation, ContextChargedForProgram,
    Ext,
};
use gear_common::{
    auxiliary::{
        gas_provider::PlainNodeId, mailbox::MailboxErrorImpl, waitlist::WaitlistErrorImpl,
        BlockNumber,
    },
    event::{MessageWaitedReason, MessageWaitedRuntimeReason},
    scheduler::StorageType,
    storage::Interval,
    LockId, Origin,
};
use gear_core::{
    code::InstrumentedCode,
    gas_metering::{DbWeights, RentWeights, Schedule},
    ids::{prelude::*, ActorId, CodeId, MessageId, ReservationId},
    memory::PageBuf,
    message::{
        Dispatch, DispatchKind, Message, ReplyMessage, StoredDelayedDispatch, StoredDispatch,
        StoredMessage, UserMessage, UserStoredMessage,
    },
    pages::{num_traits::Zero, GearPage},
    tasks::ScheduledTask,
};
use gear_lazy_pages_native_interface::LazyPagesNative;
use hold_bound::HoldBoundBuilder;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    convert::TryInto,
    fmt::Debug,
    mem,
};

mod block_exec;
mod expend;
mod hold_bound;
mod journal;
mod memory;
mod reservations;
mod send_dispatch;
mod task;
mod wait_wake;

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
    pub(crate) first_incomplete_tasks_block: Option<u32>,
    // Last block execution info
    pub(crate) succeed: BTreeSet<MessageId>,
    pub(crate) failed: BTreeSet<MessageId>,
    pub(crate) not_executed: BTreeSet<MessageId>,
    pub(crate) gas_burned: BTreeMap<MessageId, Gas>,
    pub(crate) log: Vec<StoredMessage>,
}

impl ExtManager {
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
        program_id: ActorId,
        program: Program,
        init_message_id: Option<MessageId>,
    ) -> Option<TestActor> {
        Actors::insert(program_id, TestActor::new(init_message_id, program))
    }

    pub(crate) fn store_new_code(&mut self, code_id: CodeId, code: Vec<u8>) {
        self.opt_binaries.insert(code_id, code);
    }

    pub(crate) fn read_code(&self, code_id: CodeId) -> Option<&[u8]> {
        self.opt_binaries.get(&code_id).map(|code| code.as_ref())
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

    pub(crate) fn update_storage_pages(
        &mut self,
        program_id: &ActorId,
        memory_pages: BTreeMap<GearPage, PageBuf>,
    ) {
        Actors::modify(*program_id, |actor| {
            let pages_data = actor
                .unwrap_or_else(|| panic!("Actor id {program_id:?} not found"))
                .pages_data_mut()
                .expect("No pages data found for program");

            for (page, buf) in memory_pages {
                pages_data.insert(page, buf);
            }
        });
    }

    pub(crate) fn mint_to(&mut self, id: &ActorId, value: Value) {
        Accounts::increase(*id, value);
    }

    pub(crate) fn balance_of(&self, id: &ActorId) -> Value {
        Accounts::balance(*id)
    }

    pub(crate) fn override_balance(&mut self, &id: &ActorId, balance: Value) {
        if Actors::is_user(id) && balance < crate::EXISTENTIAL_DEPOSIT {
            usage_panic!(
                "An attempt to override balance with value ({}) less than existential deposit ({}. \
                Please try to use bigger balance value",
                balance,
                crate::EXISTENTIAL_DEPOSIT
            );
        }
        Accounts::override_balance(id, balance);
    }

    pub(crate) fn on_task_pool_change(&mut self) {
        let write = DbWeights::default().write.ref_time;
        self.gas_allowance = self.gas_allowance.saturating_sub(write);
    }

    fn init_success(&mut self, program_id: ActorId) {
        Actors::modify(program_id, |actor| {
            actor
                .unwrap_or_else(|| panic!("Actor id {program_id:?} not found"))
                .set_initialized()
        });
    }

    fn init_failure(&mut self, program_id: ActorId, origin: ActorId) {
        Actors::modify(program_id, |actor| {
            let actor = actor.unwrap_or_else(|| panic!("Actor id {program_id:?} not found"));
            *actor = TestActor::FailedInit;
        });

        let value = Accounts::balance(program_id);
        if value != 0 {
            Accounts::transfer(program_id, origin, value, false);
        }
    }

    pub(crate) fn update_program<R, F: FnOnce(&mut Program) -> R>(
        &mut self,
        id: ActorId,
        op: F,
    ) -> Option<R> {
        Actors::modify(id, |actor| {
            actor.and_then(|actor| actor.program_mut().map(op))
        })
    }

    pub(crate) fn read_mailbox_message(
        &mut self,
        to: ActorId,
        from_mid: MessageId,
    ) -> Result<UserStoredMessage, MailboxErrorImpl> {
        let (message, hold_interval) = self.mailbox.remove(to, from_mid)?;

        let expected = hold_interval.finish;

        let user_id = message.destination();
        let from = message.source();

        self.charge_for_hold(message.id(), hold_interval, StorageType::Mailbox);
        self.consume_and_retrieve(message.id());

        self.bank.transfer_value(from, user_id, message.value());

        let _ = self
            .task_pool
            .delete(
                expected,
                ScheduledTask::RemoveFromMailbox(user_id, message.id()),
            )
            .map(|_| {
                self.on_task_pool_change();
            });

        Ok(message)
    }
}
