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
    EXISTENTIAL_DEPOSIT, GAS_ALLOWANCE, GAS_MULTIPLIER, MAX_RESERVATIONS, MAX_USER_GAS_LIMIT,
    ProgramBuilder, RESERVE_FOR, Result, TestError, VALUE_PER_GAS,
    constants::{BlockNumber, Gas, Value},
    error::usage_panic,
    log::{BlockRunResult, CoreLog},
    state::{
        self,
        accounts::Accounts,
        bank::Bank,
        blocks::BlocksManager,
        gas_tree::GasTreeManager,
        mailbox::manager::{MailboxErrorImpl, MailboxManager},
        nonce::NonceManager,
        programs::{GTestProgram, ProgramsStorageManager},
        queue::QueueManager,
        stash::DispatchStashManager,
        task_pool::TaskPoolManager,
        waitlist::WaitlistManager,
    },
};
use core_processor::{Ext, common::*, configs::BlockConfig};
use gear_common::{
    LockId, Origin,
    event::{MessageWaitedReason, MessageWaitedRuntimeReason},
    gas_provider::auxiliary::PlainNodeId,
    scheduler::StorageType,
    storage::Interval,
};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    gas_metering::{DbWeights, RentWeights, Schedule},
    ids::{ActorId, CodeId, MessageId, ReservationId, prelude::*},
    memory::PageBuf,
    message::{Dispatch, DispatchKind, Message, ReplyMessage, StoredMessage, UserStoredMessage},
    pages::{GearPage, num_traits::Zero},
    program::{ActiveProgram, Program, ProgramState},
    tasks::ScheduledTask,
};
use gear_lazy_pages_native_interface::LazyPagesNative;
use hold_bound::HoldBoundBuilder;
use std::{
    collections::{BTreeMap, BTreeSet},
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

pub(crate) const CUSTOM_WASM_PROGRAM_CODE_ID: CodeId =
    CodeId::new(*b"CUSTOM_WASM_PROGRAM_CODE_ID\0\0\0\0\0");

#[derive(Debug, Default)]
pub(crate) struct ExtManager {
    // State with possible overlay
    pub(crate) blocks_manager: BlocksManager,
    pub(crate) nonce_manager: NonceManager,
    pub(crate) bank: Bank,
    pub(crate) dispatches: QueueManager,
    pub(crate) mailbox: MailboxManager,
    pub(crate) task_pool: TaskPoolManager,
    pub(crate) waitlist: WaitlistManager,
    pub(crate) gas_tree: GasTreeManager,
    pub(crate) dispatches_stash: DispatchStashManager,

    // State with no overlay
    pub(crate) gas_allowance: Gas,
    pub(crate) opt_binaries: BTreeMap<CodeId, Vec<u8>>,
    pub(crate) meta_binaries: BTreeMap<CodeId, Vec<u8>>,
    pub(crate) instrumented_codes: BTreeMap<CodeId, InstrumentedCode>,
    pub(crate) code_metadata: BTreeMap<CodeId, CodeMetadata>,
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
            blocks_manager: BlocksManager,
            messages_processing_enabled: true,
            ..Default::default()
        }
    }

    pub fn block_height(&self) -> u32 {
        self.blocks_manager.get().height
    }

    // Returns `true`, if the program was already present.
    pub(crate) fn store_program(&mut self, program_id: ActorId, program: GTestProgram) -> bool {
        ProgramsStorageManager::insert_program(program_id, program)
    }

    pub(crate) fn store_code(&mut self, code_id: CodeId, code: Vec<u8>) {
        self.opt_binaries.insert(code_id, code.clone());

        let (instrumented_code, code_metadata) =
            ProgramBuilder::build_instrumented_code_and_id(code)
                .1
                .into_parts();
        self.instrumented_codes.insert(code_id, instrumented_code);
        self.code_metadata.insert(code_id, code_metadata);
    }

    pub(crate) fn instrumented_code(&self, code_id: CodeId) -> Option<&InstrumentedCode> {
        self.instrumented_codes.get(&code_id)
    }

    pub(crate) fn code_metadata(&self, code_id: CodeId) -> Option<&CodeMetadata> {
        self.code_metadata.get(&code_id)
    }

    pub(crate) fn original_code(&self, code_id: CodeId) -> Option<&[u8]> {
        self.opt_binaries.get(&code_id).map(|code| code.as_ref())
    }

    pub(crate) fn fetch_inc_message_nonce(&mut self) -> u64 {
        self.nonce_manager.fetch_inc_message_nonce()
    }

    pub(crate) fn free_id_nonce(&mut self) -> u64 {
        let mut id_nonce = self.nonce_manager.id_nonce();
        while ProgramsStorageManager::has_program(id_nonce.into()) {
            self.nonce_manager.inc_id_nonce();
            id_nonce = self.nonce_manager.id_nonce();
        }

        id_nonce
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
            let Program::Active(active_program) = program
                .unwrap_or_else(|| panic!("Actor id {program_id:?} not found"))
                .as_primary_program_mut()
            else {
                unreachable!(
                    "Before init finishes, program must always be active. But {program_id:?} program is not active."
                );
            };

            active_program.state = ProgramState::Initialized;
        });
    }

    fn init_failure(&mut self, program_id: ActorId, origin: ActorId) {
        self.clean_waitlist(program_id);
        self.remove_gas_reservation_map(program_id);
        ProgramsStorageManager::modify_program(program_id, |program| {
            if let Some(program) = program.map(GTestProgram::as_primary_program_mut) {
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

    pub(crate) fn update_program<R, F: FnOnce(&mut ActiveProgram<BlockNumber>) -> R>(
        &mut self,
        id: ActorId,
        op: F,
    ) -> Option<R> {
        ProgramsStorageManager::modify_program(id, |program| {
            program.and_then(|program| {
                if let Program::Active(active_program) = program.as_primary_program_mut() {
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
    /// Enables the overlay mode for gear-runtime emulating storages
    /// (auxiliaries and internal ones).
    pub(crate) fn enable_overlay(&self) {
        state::enable_overlay();
    }

    /// Disables the overlay mode for gear-runtime emulating storages
    /// (auxiliaries and internal ones).
    pub(crate) fn disable_overlay(&self) {
        state::disable_overlay();
    }
}
