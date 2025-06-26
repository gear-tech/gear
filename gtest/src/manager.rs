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
        accounts::Accounts, bank::Bank, blocks::BlocksManager, gas_tree::GasTreeManager,
        mailbox::MailboxManager, programs::ProgramsStorageManager, task_pool::TaskPoolManager,
        waitlist::WaitlistManager,
    },
    Block, ProgramBuilder, Result, TestError, EPOCH_DURATION_IN_BLOCKS, EXISTENTIAL_DEPOSIT,
    GAS_ALLOWANCE, GAS_MULTIPLIER, INITIAL_RANDOM_SEED, MAX_RESERVATIONS, MAX_USER_GAS_LIMIT,
    RESERVE_FOR, VALUE_PER_GAS,
};
use core_processor::{common::*, configs::BlockConfig, ContextChargedForInstrumentation, Ext};
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
    program::{ActiveProgram, Program, ProgramState},
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
    pub(crate) instrumented_codes: BTreeMap<CodeId, InstrumentedCode>,
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
    pub(crate) no_code_program: BTreeSet<ActorId>,
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
        program: Program<Block>,
    ) -> Option<Program<Block>> {
        ProgramsStorageManager::insert_program(program_id, program)
    }

    pub(crate) fn store_new_code(&mut self, code_id: CodeId, code: Vec<u8>) {
        self.opt_binaries.insert(code_id, code.clone());

        let (instrumented_code, _) =
            ProgramBuilder::build_instrumented_code_and_id(code).into_parts();
        self.instrumented_codes.insert(code_id, instrumented_code);
    }

    pub(crate) fn instrumented_code(&self, code_id: CodeId) -> Option<&InstrumentedCode> {
        self.instrumented_codes.get(&code_id)
    }

    pub(crate) fn original_code(&self, code_id: CodeId) -> Option<&[u8]> {
        self.opt_binaries.get(&code_id).map(|code| code.as_ref())
    }

    fn original_code_size(&self, code_id: CodeId) -> Option<usize> {
        self.opt_binaries.get(&code_id).map(|code| code.len())
    }

    pub(crate) fn fetch_inc_message_nonce(&mut self) -> u64 {
        let nonce = self.msg_nonce;
        self.msg_nonce += 1;
        nonce
    }

    pub(crate) fn free_id_nonce(&mut self) -> u64 {
        while ProgramsStorageManager::has_program(self.id_nonce.into()) {
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
        program_id: ActorId,
        memory_pages: BTreeMap<GearPage, PageBuf>,
    ) {
        for (page, buf) in memory_pages {
            ProgramsStorageManager::set_program_page(program_id, page, buf);
        }
    }

    pub(crate) fn mint_to(&mut self, id: ActorId, value: Value) {
        Accounts::increase(id, value);
    }

    pub(crate) fn balance_of(&self, id: ActorId) -> Value {
        Accounts::balance(id)
    }

    pub(crate) fn override_balance(&mut self, id: ActorId, balance: Value) {
        if ProgramsStorageManager::is_user(id) && balance < crate::EXISTENTIAL_DEPOSIT {
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
        ProgramsStorageManager::modify_program(program_id, |program| {
            let Program::Active(active_program) =
                program.unwrap_or_else(|| panic!("Actor id {program_id:?} not found"))
            else {
                unreachable!("Before init finishes, program must always be active. But {program_id:?} program is not active.");
            };

            active_program.state = ProgramState::Initialized;
        });
    }

    fn init_failure(&mut self, program_id: ActorId, origin: ActorId) {
        self.clean_waitlist(program_id);
        self.remove_gas_reservation_map(program_id);
        ProgramsStorageManager::modify_program(program_id, |program| {
            if let Some(program) = program {
                if !program.is_active() {
                    // Guaranteed to be called only on active program
                    unreachable!(
                        "ExtManager::init_failure: failed to exit active program. \
                    Program - {program_id}, actual program - {program:?}"
                    );
                }

                *program = Program::Terminated(program_id);
            } else {
                // That's a case if no code exists for the program
                // requested to be created from another program and
                // there was not enough gas to get program from storage.
                log::debug!("Failed init is set for non-existing actor");
            }
        });

        let value = Accounts::balance(program_id);
        if value != 0 {
            Accounts::transfer(program_id, origin, value, false);
        }
    }

    pub(crate) fn update_program<R, F: FnOnce(&mut ActiveProgram<Block>) -> R>(
        &mut self,
        id: ActorId,
        op: F,
    ) -> Option<R> {
        ProgramsStorageManager::modify_program(id, |program| {
            program.and_then(|actor| {
                if let Program::Active(active_program) = actor {
                    Some(op(active_program))
                } else {
                    None
                }
            })
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

    pub(crate) fn clean_waitlist(&mut self, id: ActorId) {
        self.waitlist.drain_key(id).for_each(|entry| {
            let message = self.wake_dispatch_requirements(entry);

            self.dispatches.push_back(message);
        });
    }
}
