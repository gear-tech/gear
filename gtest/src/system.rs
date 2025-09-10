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

use crate::{
    GAS_ALLOWANCE, Gas, Value,
    artifacts::{BlockRunResult, UserMessageEvent},
    error::usage_panic,
    manager::ExtManager,
    program::{Program, ProgramIdWrapper},
    state::{
        accounts::Accounts, bridge::BridgeBuiltinStorage, mailbox::UserMailbox,
        programs::ProgramsStorageManager,
    },
};
use core_processor::common::JournalNote;
use gear_common::MessageId;
use gear_core::{
    ids::{
        ActorId, CodeId,
        prelude::{CodeIdExt, MessageIdExt},
    },
    message::{Dispatch, DispatchKind, Message, ReplyDetails},
    pages::GearPage,
    program::Program as PrimaryProgram,
    rpc::ReplyInfo,
};
use gear_lazy_pages::{LazyPagesStorage, LazyPagesVersion};
use gear_lazy_pages_common::LazyPagesInitContext;
use parity_scale_codec::{Decode, DecodeAll};
use path_clean::PathClean;
use std::{borrow::Cow, cell::RefCell, env, fs, mem, panic, path::Path};
use tracing_subscriber::EnvFilter;

thread_local! {
    /// `System` is a singleton with a one instance and no copies returned.
    ///
    /// `OnceCell` is used to control one-time instantiation, while `RefCell`
    /// is needed for interior mutability to uninitialize the global.
    static SYSTEM_INITIALIZED: RefCell<bool> = const { RefCell::new(false) };
}

#[derive(Decode)]
struct PageKey {
    _page_storage_prefix: [u8; 32],
    program_id: ActorId,
    _memory_infix: u32,
    page: GearPage,
}

#[derive(Debug)]
struct PagesStorage;

impl LazyPagesStorage for PagesStorage {
    fn page_exists(&self, mut key: &[u8]) -> bool {
        let PageKey {
            program_id, page, ..
        } = PageKey::decode_all(&mut key).expect("Invalid key");

        ProgramsStorageManager::program_page(program_id, page).is_some()
    }

    fn load_page(&mut self, mut key: &[u8], buffer: &mut [u8]) -> Option<u32> {
        let PageKey {
            program_id, page, ..
        } = PageKey::decode_all(&mut key).expect("Invalid key");

        ProgramsStorageManager::program_page(program_id, page).map(|page_buf| {
            buffer.copy_from_slice(page_buf.as_ref());
            page_buf.len() as u32
        })
    }
}

/// Gear blockchain environment simulator.
///
/// The type manages the state of the blockchain, also provides
/// various utilities for interacting with it.
pub struct System(pub(crate) RefCell<ExtManager>);

impl System {
    /// Prefix for lazy pages.
    pub(crate) const PAGE_STORAGE_PREFIX: [u8; 32] = *b"gtestgtestgtestgtestgtestgtest00";

    /// Create a new testing environment.
    ///
    /// # Panics
    /// Only one instance of the `System` in the current thread is possible to
    /// create. Instantiation of the other one leads to a runtime panic.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        SYSTEM_INITIALIZED.with_borrow_mut(|initialized| {
            if *initialized {
                panic!("Impossible to have multiple instances of the `System`.");
            }

            let ext_manager = ExtManager::new();
            gear_lazy_pages::init(
                LazyPagesVersion::Version1,
                LazyPagesInitContext::new(Self::PAGE_STORAGE_PREFIX),
                PagesStorage,
            )
            .expect("Failed to init lazy-pages");

            *initialized = true;

            Self(RefCell::new(ext_manager))
        })
    }

    /// Init logger with "gwasm" target set to `debug` level.
    ///
    /// If `RUST_LOG` environment variable is set, it will be used as the filter.
    pub fn init_logger(&self) {
        self.init_logger_with_default_filter("gwasm=debug");
    }

    /// Init logger with "gwasm" and "gtest" targets set to `debug` level.
    ///
    /// If `RUST_LOG` environment variable is set, it will be used as the filter.
    pub fn init_verbose_logger(&self) {
        self.init_logger_with_default_filter("gwasm=debug,gtest=debug");
    }

    /// Init logger with `default_filter` as default filter.
    ///
    /// If `RUST_LOG` environment variable is set, it will be used as the filter.
    pub fn init_logger_with_default_filter<'a>(&self, default_filter: impl Into<Cow<'a, str>>) {
        let filter = if env::var(EnvFilter::DEFAULT_ENV).is_ok() {
            EnvFilter::from_default_env()
        } else {
            EnvFilter::new(default_filter.into())
        };
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .without_time()
            .with_thread_names(true)
            .try_init();
    }

    /// Returns amount of messages in the queue.
    pub fn queue_len(&self) -> usize {
        self.0.borrow().dispatches.len()
    }

    /// Run next block.
    ///
    /// Block execution model is the following:
    /// - increase the block number, update the timestamp
    /// - process tasks from the task pool
    /// - process messages in the queue.
    ///
    /// The system is always initialized with a 0 block number. Current block
    /// number in the system is the number of the already executed block,
    /// therefore block execution starts with a block info update (block
    /// number increase, timestamp update). For example, if current block
    /// number is 2, it means that messages and tasks on 2 were executed, so
    /// the method goes to block number 3 and executes tasks and messages for
    /// the updated block number.
    ///
    /// Task processing basically tries to execute the scheduled to the specific
    /// block tasks:
    /// - delayed sending
    /// - waking message
    /// - removing from the mailbox
    /// - removing reservations
    /// - removing stalled wait message.
    ///
    /// Messages processing executes messages until either queue becomes empty
    /// or block gas allowance is fully consumed.
    pub fn run_next_block(&self) -> BlockRunResult {
        self.run_next_block_with_allowance(GAS_ALLOWANCE)
    }

    /// Runs blocks same as [`Self::run_next_block`], but with limited
    /// block gas allowance.
    pub fn run_next_block_with_allowance(&self, allowance: Gas) -> BlockRunResult {
        if allowance > GAS_ALLOWANCE {
            usage_panic!(
                "Provided allowance more than allowed limit of {GAS_ALLOWANCE}. \
                Please, provide an allowance less than or equal to the limit."
            );
        }

        self.0.borrow_mut().run_new_block(allowance)
    }

    /// Runs blocks same as [`Self::run_next_block`], but executes blocks to
    /// block number `bn` including it.
    pub fn run_to_block(&self, bn: u32) -> Vec<BlockRunResult> {
        let mut manager = self.0.borrow_mut();

        let mut current_block = manager.block_height();
        if current_block > bn {
            usage_panic!("Can't run blocks until bn {bn}, as current bn is {current_block}");
        }

        let mut ret = Vec::with_capacity((bn - current_block) as usize);
        while current_block != bn {
            let res = manager.run_new_block(GAS_ALLOWANCE);
            ret.push(res);

            current_block = manager.block_height();
        }

        ret
    }

    /// Runs `amount` of blocks only with processing task pool, without
    /// processing the message queue.
    pub fn run_scheduled_tasks(&self, amount: u32) -> Vec<BlockRunResult> {
        let mut manager = self.0.borrow_mut();
        let block_height = manager.block_height();

        (block_height..block_height + amount)
            .map(|_| {
                let block_info = manager.blocks_manager.next_block();
                let next_block_number = block_info.height;
                manager.process_tasks(next_block_number);

                let events = mem::take(&mut manager.events)
                    .into_iter()
                    .map(UserMessageEvent::from)
                    .collect();
                BlockRunResult {
                    block_info,
                    gas_allowance_spent: GAS_ALLOWANCE - manager.gas_allowance,
                    events,
                    ..Default::default()
                }
            })
            .collect()
    }

    /// Return the current block height of the testing environment.
    pub fn block_height(&self) -> u32 {
        self.0.borrow().block_height()
    }

    /// Return the current block timestamp of the testing environment.
    pub fn block_timestamp(&self) -> u64 {
        self.0.borrow().blocks_manager.get().timestamp
    }

    /// Returns a [`Program`] by `id`.
    pub fn get_program<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> Option<Program<'_>> {
        let id = id.into().0;
        if ProgramsStorageManager::is_program(id) {
            Some(Program {
                id,
                manager: &self.0,
            })
        } else {
            None
        }
    }

    /// Returns last added program.
    pub fn last_program(&self) -> Option<Program<'_>> {
        self.programs().into_iter().next_back()
    }

    /// Returns a list of all known programs, stored and managed by the `System` instance.
    pub fn programs(&self) -> Vec<Program<'_>> {
        ProgramsStorageManager::program_ids()
            .into_iter()
            .map(|id| Program {
                id,
                manager: &self.0,
            })
            .collect()
    }

    /// Detects if a program is active.
    ///
    /// An active program means that the program receive messages.
    /// If `false` is returned, it means that the program has
    /// exited or terminated that it can't be called anymore.
    pub fn is_active_program<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> bool {
        let program_id = id.into().0;
        ProgramsStorageManager::is_active_program(program_id)
    }

    /// Returns id of the inheritor actor if program exited.
    ///
    /// Otherwise, returns `None`.
    pub fn inheritor_of<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> Option<ActorId> {
        let program_id = id.into().0;
        ProgramsStorageManager::access_primary_program(program_id, |program| {
            program.and_then(|program| {
                if let PrimaryProgram::Exited(inheritor_id) = program {
                    Some(*inheritor_id)
                } else {
                    None
                }
            })
        })
    }

    /// Saves code to the storage and returns its code hash
    ///
    /// Same as [`System::submit_code_file`], but the path is provided as relative to
    /// the current directory.
    pub fn submit_local_code_file<P: AsRef<Path>>(&self, code_path: P) -> CodeId {
        let path = env::current_dir()
            .expect("Unable to get root directory of the project")
            .join(code_path)
            .clean();

        self.submit_code_file(path)
    }

    /// Saves code from file to the storage and returns its code hash.
    ///
    /// See also [`System::submit_code`]
    pub fn submit_code_file<P: AsRef<Path>>(&self, code_path: P) -> CodeId {
        let code = fs::read(&code_path).unwrap_or_else(|_| {
            usage_panic!(
                "Failed to read file {}",
                code_path.as_ref().to_string_lossy()
            )
        });

        self.submit_code(code)
    }

    /// Saves code to the storage and returns its code hash
    ///
    /// This method is mainly used for providing a proper program from program
    /// creation logic. In order to successfully create a new program with
    /// `gstd::prog::create_program_bytes_with_gas` function, developer should
    /// provide to the function "child's" code hash. Code for that code hash
    /// must be in storage at the time of the `gstd` function call. So this method
    /// stores the code in storage.
    ///
    /// Note: method saves instrumented version of the code.
    pub fn submit_code(&self, binary: impl Into<Vec<u8>>) -> CodeId {
        let code = binary.into();
        let code_id = CodeId::generate(code.as_ref());

        // Save original code
        self.0.borrow_mut().store_code(code_id, code);

        code_id
    }

    /// Returns previously submitted original code finding it by its code hash.
    pub fn submitted_code(&self, code_id: CodeId) -> Option<Vec<u8>> {
        self.0
            .borrow()
            .original_code(code_id)
            .map(|code| code.to_vec())
    }

    /// Extract mailbox of user with given `id`.
    ///
    /// The mailbox contains messages from the program that are waiting
    /// for user action.
    pub fn get_mailbox<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> UserMailbox<'_> {
        let program_id = id.into().0;
        if !ProgramsStorageManager::is_user(program_id) {
            usage_panic!("Mailbox available only for users. Please, provide a user id.");
        }
        UserMailbox::new(program_id, &self.0)
    }

    /// Mint to user with given `id` a value in amount of `value`.
    ///
    /// # Panics
    /// Panics if `id` is a program id.
    pub fn mint_to<ID: Into<ProgramIdWrapper>>(&self, id: ID, value: Value) {
        let id = id.into().0;

        if ProgramsStorageManager::is_program(id) {
            usage_panic!(
                "Attempt to mint value to a program {id:?}. Please, use `System::transfer` instead"
            );
        }

        self.0.borrow_mut().mint_to(id, value);
    }

    /// Transfer balance from user with given `from` id to user with given `to`
    /// id.
    ///
    /// # Panics
    /// Panics if `from` is a program id.
    pub fn transfer(
        &self,
        from: impl Into<ProgramIdWrapper>,
        to: impl Into<ProgramIdWrapper>,
        value: Value,
        keep_alive: bool,
    ) {
        let from = from.into().0;
        let to = to.into().0;

        if ProgramsStorageManager::is_program(from) {
            usage_panic!(
                "Attempt to transfer from a program {from:?}. Please, provide `from` user id."
            );
        }

        Accounts::transfer(from, to, value, keep_alive);
    }

    /// Returns balance of user with given `id`.
    pub fn balance_of<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> Value {
        let actor_id = id.into().0;
        self.0.borrow().balance_of(actor_id)
    }

    /// Calculate reply that would be received when sending
    /// message to initialized program with any of `Program::send*` methods.
    pub fn calculate_reply_for_handle(
        &self,
        origin: impl Into<ProgramIdWrapper>,
        destination: impl Into<ProgramIdWrapper>,
        payload: impl Into<Vec<u8>>,
        gas_limit: u64,
        value: Value,
    ) -> Result<ReplyInfo, String> {
        let mut manager_mut = self.0.borrow_mut();

        // Enter the overlay mode
        manager_mut.enable_overlay();

        // Clear the queue
        manager_mut.dispatches.clear();

        let origin = origin.into().0;
        let destination = destination.into().0;
        let payload = payload
            .into()
            .try_into()
            .expect("failed to convert payload to limited payload");

        // Prepare the message
        let block_number = manager_mut.block_height() + 1;
        let message = Message::new(
            MessageId::generate_from_user(
                block_number,
                origin,
                manager_mut.fetch_inc_message_nonce() as u128,
            ),
            origin,
            destination,
            payload,
            Some(gas_limit),
            value,
            None,
        );

        if !manager_mut.is_builtin(destination)
            && !ProgramsStorageManager::is_active_program(destination)
        {
            usage_panic!("Actor with {destination} id is not executable");
        }

        let dispatch = Dispatch::new(DispatchKind::Handle, message);

        // Validate and route the dispatch
        let message_id = manager_mut.validate_and_route_dispatch(dispatch);

        // Run queue for reply to the `message_id`.
        let block_config = manager_mut.block_config();

        while let Some(dispatch) = manager_mut.dispatches.pop_front() {
            // For testing purposes, we set the gas allowance to the maximum for each
            // message
            manager_mut.gas_allowance = GAS_ALLOWANCE;
            // No need to check the flag after the execution, as we give infinite
            // allowance for the reply calculation.
            manager_mut.messages_processing_enabled = true;

            // Process the dispatch and obtain the journal.
            let journal = manager_mut.process_dispatch(&block_config, dispatch);

            // Search for the reply in the journal.
            for note in &journal {
                let JournalNote::SendDispatch { dispatch, .. } = note else {
                    continue;
                };

                if let Some(code) = dispatch
                    .reply_details()
                    .map(ReplyDetails::into_parts)
                    .and_then(|(replied_to, code)| replied_to.eq(&message_id).then_some(code))
                {
                    // Before any return from the function, overlay must be disabled.
                    manager_mut.disable_overlay();

                    return Ok(ReplyInfo {
                        payload: dispatch.payload_bytes().to_vec(),
                        value: dispatch.value(),
                        code,
                    });
                }
            }

            // As long as no reply was found, we need to handle the journal.
            core_processor::handle_journal(journal, &mut *manager_mut);
        }

        // Before any return from the function, overlay must be disabled.
        manager_mut.disable_overlay();

        Err(String::from("Queue is empty, but reply wasn't found"))
    }
}

impl Drop for System {
    fn drop(&mut self) {
        // Uninitialize
        SYSTEM_INITIALIZED.with_borrow_mut(|initialized| *initialized = false);
        let manager = self.0.borrow();
        manager.gas_tree.clear();
        manager.mailbox.clear();
        manager.task_pool.clear();
        manager.waitlist.clear();
        manager.blocks_manager.reset();
        manager.bank.clear();
        manager.nonce_manager.reset();
        manager.dispatches.clear();
        manager.dispatches_stash.clear();

        // Clear programs and accounts storages
        ProgramsStorageManager::clear();
        Accounts::clear();

        // Clear bridge-builtins state
        BridgeBuiltinStorage::clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEFAULT_USER_ALICE, EXISTENTIAL_DEPOSIT, EventBuilder, MAX_USER_GAS_LIMIT};
    use gear_core_errors::{ReplyCode, SuccessReplyReason};

    #[test]
    #[should_panic(expected = "Impossible to have multiple instances of the `System`.")]
    fn test_system_being_singleton() {
        let _first_instance = System::new();

        let _second_instance = System::new();
    }

    #[test]
    fn test_multithread_copy_singleton() {
        let first_instance = System::new();
        first_instance.run_scheduled_tasks(5);

        assert_eq!(first_instance.block_height(), 5);

        let h = std::thread::spawn(|| {
            let second_instance = System::new();

            second_instance.run_scheduled_tasks(10);
            assert_eq!(second_instance.block_height(), 10);
        });

        h.join().expect("internal error failed joining thread");

        assert_eq!(first_instance.block_height(), 5);
    }

    #[test]
    fn test_bn_adjustments() {
        let sys = System::new();
        assert_eq!(sys.block_height(), 0);

        // ### Check block info after run to next block ###
        let res = sys.run_next_block();
        let block_info = res.block_info;
        assert_eq!(block_info.height, sys.block_height());
        assert_eq!(block_info.height, 1);

        // ### Check block info after run to block ###
        let current_height = block_info.height;
        let until_height = 5;
        let results = sys.run_to_block(until_height);
        assert_eq!(results.len(), (until_height - current_height) as usize);

        // Check first block executed is always the next block
        let first_run = results.first().expect("checked above");
        assert_eq!(first_run.block_info.height, current_height + 1);

        // Check the last block executed number
        let last_run = results.last().expect("checked above");
        assert_eq!(last_run.block_info.height, until_height);
        assert_eq!(last_run.block_info.height, sys.block_height());

        // ### Check block info after running the task pool ###
        let current_height = last_run.block_info.height;
        let amount_of_blocks = 10;
        let results = sys.run_scheduled_tasks(amount_of_blocks);
        assert_eq!(results.len(), amount_of_blocks as usize);

        let first_run = results.first().expect("checked above");
        assert_eq!(first_run.block_info.height, current_height + 1);

        let last_run = results.last().expect("checked above");
        assert_eq!(
            last_run.block_info.height,
            current_height + amount_of_blocks
        );

        assert_eq!(last_run.block_info.height, 15);
    }

    #[test]
    #[should_panic(expected = "Got message sent to incomplete user program")]
    fn panic_calculate_reply_no_actor() {
        let sys = System::new();

        let origin = DEFAULT_USER_ALICE;
        let pid = 42;
        let ping_program = Program::from_binary_with_id(&sys, pid, demo_ping::WASM_BINARY);
        let destination = ping_program.id();

        // Try send calculate reply for handle.
        // Must fail because the program is not initialized.
        let _ = sys.calculate_reply_for_handle(
            origin,
            destination,
            b"PING".to_vec(),
            MAX_USER_GAS_LIMIT,
            0,
        );
    }

    #[test]
    fn test_calculate_reply_for_handle() {
        use demo_piggy_bank::WASM_BINARY;

        let sys = System::new();

        let program = Program::from_binary_with_id(&sys, 42, WASM_BINARY);
        let pid = program.id();

        // Initialize the program
        let init_mid = program.send_bytes(DEFAULT_USER_ALICE, b"");
        let block_result = sys.run_next_block();
        assert!(block_result.succeed.contains(&init_mid));

        let program_balance_before_overlay = sys.balance_of(pid);
        let alice_balance_before_overlay = sys.balance_of(DEFAULT_USER_ALICE);
        let reply_info = sys
            .calculate_reply_for_handle(
                DEFAULT_USER_ALICE,
                pid,
                b"",
                MAX_USER_GAS_LIMIT,
                EXISTENTIAL_DEPOSIT * 10,
            )
            .expect("Failed to calculate reply for handle");
        assert_eq!(
            reply_info.code,
            ReplyCode::Success(SuccessReplyReason::Auto)
        );

        // Check that overlay didn't change the state
        assert_eq!(sys.balance_of(pid), program_balance_before_overlay);
        assert_eq!(
            sys.balance_of(DEFAULT_USER_ALICE),
            alice_balance_before_overlay
        );

        // Send message with value
        let storing_value = EXISTENTIAL_DEPOSIT * 10;
        let handle_mid1 = program.send_bytes_with_value(DEFAULT_USER_ALICE, b"", storing_value);
        let block_result = sys.run_next_block();
        assert!(block_result.succeed.contains(&handle_mid1));
        assert!(
            block_result.contains(
                &EventBuilder::new()
                    .with_destination(DEFAULT_USER_ALICE)
                    .with_reply_code(reply_info.code)
            )
        );

        let alice_expected_balance_after_msg1 =
            alice_balance_before_overlay - storing_value - block_result.spent_value();

        assert_eq!(
            sys.balance_of(pid),
            program_balance_before_overlay + storing_value
        );
        assert_eq!(
            sys.balance_of(DEFAULT_USER_ALICE),
            alice_expected_balance_after_msg1
        );

        let reply_info = sys
            .calculate_reply_for_handle(DEFAULT_USER_ALICE, pid, b"smash", MAX_USER_GAS_LIMIT, 0)
            .expect("Failed to calculate reply for handle");
        assert_eq!(
            reply_info.code,
            ReplyCode::Success(SuccessReplyReason::Auto)
        );

        // Check that overlay didn't change the state
        assert_eq!(
            sys.balance_of(pid),
            program_balance_before_overlay + storing_value
        );
        assert_eq!(
            sys.balance_of(DEFAULT_USER_ALICE),
            alice_expected_balance_after_msg1
        );
        let mailbox = sys.get_mailbox(DEFAULT_USER_ALICE);
        let event = EventBuilder::new()
            .with_destination(DEFAULT_USER_ALICE)
            .with_payload_bytes(b"send");
        assert!(!mailbox.contains(&event));

        let handle_mid = program.send_bytes(DEFAULT_USER_ALICE, b"smash");
        let block_result = sys.run_next_block();
        assert!(block_result.succeed.contains(&handle_mid));
        assert_eq!(sys.balance_of(pid), EXISTENTIAL_DEPOSIT);
        let mailbox = sys.get_mailbox(DEFAULT_USER_ALICE);
        let event = EventBuilder::new()
            .with_destination(DEFAULT_USER_ALICE)
            .with_payload_bytes(b"send");
        assert!(mailbox.contains(&event));

        mailbox.claim_value(event).expect("Failed to claim value");

        let alice_expected_balance_after_msg2 =
            alice_expected_balance_after_msg1 - block_result.spent_value() + storing_value;
        assert_eq!(
            sys.balance_of(DEFAULT_USER_ALICE),
            alice_expected_balance_after_msg2
        );
    }
}
