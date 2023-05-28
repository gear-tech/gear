// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
    log::RunResult,
    mailbox::Mailbox,
    manager::{Balance, ExtManager},
    program::{Program, ProgramIdWrapper},
};
use colored::Colorize;
use env_logger::{Builder, Env};
use gear_common::scheduler::TaskHandler;
use gear_core::{
    ids::{CodeId, MessageId},
    message::Dispatch,
};
use path_clean::PathClean;
use std::{cell::RefCell, env, fs, io::Write, path::Path, thread};

pub struct System(pub(crate) RefCell<ExtManager>);

impl Default for System {
    fn default() -> Self {
        Self(RefCell::new(ExtManager::new()))
    }
}

impl System {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn init_logger(&self) {
        let _ = Builder::from_env(Env::default().default_filter_or("gwasm=debug"))
            .format(|buf, record| {
                let lvl = record.level().to_string().to_uppercase();
                let target = record.target().to_string();
                let mut msg = record.args().to_string();

                if target == "gwasm" {
                    msg = msg.replacen("DEBUG: ", "", 1);

                    writeln!(
                        buf,
                        "[{} {}] {}",
                        lvl.blue(),
                        thread::current().name().unwrap_or("unknown").white(),
                        msg.white()
                    )
                } else {
                    writeln!(
                        buf,
                        "[{} {}] {}",
                        target.red(),
                        thread::current().name().unwrap_or("unknown").white(),
                        msg.white()
                    )
                }
            })
            .format_target(false)
            .format_timestamp(None)
            .try_init();
    }

    pub fn send_dispatch(&self, dispatch: Dispatch) -> RunResult {
        self.0.borrow_mut().validate_and_run_dispatch(dispatch)
    }

    pub fn spend_blocks(&self, amount: u32) -> Vec<RunResult> {
        let mut manager = self.0.borrow_mut();

        (manager.block_info.height..manager.block_info.height + amount)
            .map(|_| {
                manager.check_epoch();

                let next_block_number = manager.block_info.height + 1;
                manager.block_info.height = next_block_number;
                manager.block_info.timestamp += 1000;
                manager.process_delayed_dispatches(next_block_number)
            })
            .collect::<Vec<Vec<_>>>()
            .concat()
    }

    /// Return the current block height.
    pub fn block_height(&self) -> u32 {
        self.0.borrow().block_info.height
    }

    /// Return the current block timestamp.
    pub fn block_timestamp(&self) -> u64 {
        self.0.borrow().block_info.timestamp
    }

    /// Returns a [`Program`] by `id`.
    ///
    /// The method doesn't check whether program exists or not.
    /// So if provided `id` doesn't belong to program, message sent
    /// to such "program" will cause panics.
    pub fn get_program<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> Program {
        let id = id.into().0;
        Program {
            id,
            manager: &self.0,
        }
    }

    pub fn is_active_program<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> bool {
        let program_id = id.into().0;
        !self.0.borrow().is_user(&program_id)
    }

    /// Pause program action.
    /// # Panics
    /// This method will panic if program id is incorrect or program is not initialized.
    pub fn pause_program<ID: Into<ProgramIdWrapper>>(&self, id: ID) {
        self.0.borrow_mut().pause_program(id.into().0);
    }

    /// Sends wake message to program.
    /// This method removes message from waitlist and adds it to dispatches.
    /// # Panics
    /// This method will panic if program id is incorrect or message id is incorrect.
    pub fn wake_message<ID: Into<ProgramIdWrapper>>(&self, program_id: ID, message_id: MessageId) {
        self.0
            .borrow_mut()
            .wake_message(program_id.into().0, message_id);
    }

    /// Saves code to the storage and returns it's code hash
    ///
    /// This method is mainly used for providing a proper program from program
    /// creation logic. In order to successfully create a new program with
    /// `gstd::prog::create_program_with_gas` function, developer should
    /// provide to the function "child's" code hash. Code for that code hash
    /// must be in storage at the time of the function call. So this method
    /// stores the code in storage.
    pub fn submit_code<P: AsRef<Path>>(&self, code_path: P) -> CodeId {
        let path = env::current_dir()
            .expect("Unable to get root directory of the project")
            .join(code_path)
            .clean();

        let code = fs::read(&path).unwrap_or_else(|_| panic!("Failed to read file {:?}", path));
        self.0.borrow_mut().store_new_code(&code)
    }

    pub fn get_mailbox<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> Mailbox {
        let program_id = id.into().0;
        if !self.0.borrow().is_user(&program_id) {
            panic!("Mailbox available only for users");
        }
        self.0
            .borrow_mut()
            .mailbox
            .entry(program_id)
            .or_insert_with(Vec::default);
        Mailbox::new(program_id, &self.0)
    }

    /// Removes message from the mailbox.
    /// # Panics
    /// Panics if there is no mailbox for the provided `id`.
    pub fn remove_from_mailbox<ID: Into<ProgramIdWrapper>>(
        &self,
        user_id: ID,
        message_id: MessageId,
    ) {
        let user_id = user_id.into().0;
        self.0.borrow_mut().remove_from_mailbox(user_id, message_id);
    }

    pub fn remove_from_waitlist<ID: Into<ProgramIdWrapper>>(
        &self,
        user_id: ID,
        message_id: MessageId,
    ) {
        let user_id = user_id.into().0;
        self.0
            .borrow_mut()
            .remove_from_waitlist(user_id, message_id);
    }

    /// Add value to the actor.
    pub fn mint_to<ID: Into<ProgramIdWrapper>>(&self, id: ID, value: Balance) {
        let actor_id = id.into().0;
        self.0.borrow_mut().mint_to(&actor_id, value);
    }

    /// Return actor balance (value) if exists.
    pub fn balance_of<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> Balance {
        let actor_id = id.into().0;
        self.0.borrow().balance_of(&actor_id)
    }

    /// Claim the user's value from the mailbox.
    pub fn claim_value_from_mailbox<ID: Into<ProgramIdWrapper>>(&self, id: ID) {
        let actor_id = id.into().0;
        self.0.borrow_mut().claim_value_from_mailbox(&actor_id);
    }
}
