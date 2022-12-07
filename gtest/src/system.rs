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
    EPOCH_DURATION_IN_BLOCKS, INITIAL_RANDOM_SEED,
};
use colored::Colorize;
use env_logger::{Builder, Env};
use gear_core::{ids::CodeId, message::Dispatch};
use path_clean::PathClean;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use std::{cell::RefCell, collections::HashMap, env, fs, io::Write, path::Path, thread};

pub struct System {
    pub(crate) ext: RefCell<ExtManager>,
    message_queue: HashMap<u32, Vec<Dispatch>>,
}

impl Default for System {
    fn default() -> Self {
        Self {
            ext: RefCell::new(ExtManager::new()),
            message_queue: Default::default(),
        }
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
        self.ext.borrow_mut().run_dispatch(dispatch)
    }

    /// Returns false when block has already been processed.
    ///
    /// Queues dispatch messages into message queue, process and remove
    /// when [`spend_blocks`] reaches `block_height`.
    pub fn delay_dispatch(&mut self, block_height: u32, dispatch: Dispatch) -> bool {
        if block_height <= self.block_height() {
            return false;
        }

        let maybe_queue = self.message_queue.get_mut(&block_height);
        if let Some(queue) = maybe_queue {
            queue.push(dispatch);
        } else {
            self.message_queue.insert(block_height, vec![dispatch]);
        }

        true
    }

    pub fn spend_blocks(&self, amount: u32) {
        let mut manager = self.ext.borrow_mut();
        if manager.block_info.height % EPOCH_DURATION_IN_BLOCKS == 0 {
            let mut rng = StdRng::seed_from_u64(
                INITIAL_RANDOM_SEED + (manager.block_info.height / EPOCH_DURATION_IN_BLOCKS) as u64,
            );
            let mut random = [0u8; 32];
            rng.fill_bytes(&mut random);

            manager.random_data = (random.to_vec(), manager.block_info.height + 1);
        }
        manager.block_info.height += amount;
        manager.block_info.timestamp += 1000 * amount as u64;
    }

    /// Return the current block height.
    pub fn block_height(&self) -> u32 {
        self.ext.borrow().block_info.height
    }

    /// Return the current block timestamp.
    pub fn block_timestamp(&self) -> u64 {
        self.ext.borrow().block_info.timestamp
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
            manager: &self.ext,
        }
    }

    pub fn is_active_program<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> bool {
        let program_id = id.into().0;
        !self.ext.borrow().is_user(&program_id)
    }

    /// Saves code to the storage and returns it's code hash
    ///
    /// This method is mainly used for providing a proper program from program creation logic.
    /// In order to successfully create a new program with `gstd::prog::create_program_with_gas`
    /// function, developer should provide to the function "child's" code hash. Code for that
    /// code hash must be in storage at the time of the function call. So this method stores
    /// the code in storage.
    pub fn submit_code<P: AsRef<Path>>(&self, code_path: P) -> CodeId {
        let path = env::current_dir()
            .expect("Unable to get root directory of the project")
            .join(code_path)
            .clean();

        let code = fs::read(&path).unwrap_or_else(|_| panic!("Failed to read file {:?}", path));
        self.ext.borrow_mut().store_new_code(&code)
    }

    pub fn get_mailbox<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> Mailbox {
        let program_id = id.into().0;
        if !self.ext.borrow().is_user(&program_id) {
            panic!("Mailbox available only for users");
        }
        self.ext
            .borrow_mut()
            .mailbox
            .entry(program_id)
            .or_insert_with(Vec::default);
        Mailbox::new(program_id, &self.ext)
    }

    /// Add value to the actor.
    pub fn mint_to<ID: Into<ProgramIdWrapper>>(&self, id: ID, value: Balance) {
        let actor_id = id.into().0;
        self.ext.borrow_mut().mint_to(&actor_id, value);
    }

    /// Return actor balance (value) if exists.
    pub fn balance_of<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> Balance {
        let actor_id = id.into().0;
        self.ext.borrow().balance_of(&actor_id)
    }

    /// Claim the user's value from the mailbox.
    pub fn claim_value_from_mailbox<ID: Into<ProgramIdWrapper>>(&self, id: ID) {
        let actor_id = id.into().0;
        self.ext.borrow_mut().claim_value_from_mailbox(&actor_id);
    }
}
