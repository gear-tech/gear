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

use crate::{
    log::RunResult,
    mailbox::ActorMailbox,
    manager::{Actors, Balance, ExtManager, MintMode},
    program::{Program, ProgramIdWrapper},
};
use codec::{Decode, DecodeAll};
use colored::Colorize;
use env_logger::{Builder, Env};
use gear_core::{
    ids::{CodeId, ProgramId},
    message::Dispatch,
    pages::GearPage,
};
use gear_lazy_pages::{LazyPagesStorage, LazyPagesVersion};
use gear_lazy_pages_common::LazyPagesInitContext;
use path_clean::PathClean;
use std::{borrow::Cow, cell::RefCell, env, fs, io::Write, path::Path, thread};

thread_local! {
    /// `System` is a singleton with a one instance and no copies returned.
    ///
    /// `OnceCell` is used to control one-time instantiation, while `RefCell`
    /// is needed for interior mutability to uninitialize the global.
    static SYSTEM_INITIALIZED: RefCell<bool> = const { RefCell::new(false) };
}

#[derive(Decode)]
#[codec(crate = codec)]
struct PageKey {
    _page_storage_prefix: [u8; 32],
    program_id: ProgramId,
    _memory_infix: u32,
    page: GearPage,
}

#[derive(Debug)]
struct PagesStorage {
    actors: Actors,
}

impl LazyPagesStorage for PagesStorage {
    fn page_exists(&self, mut key: &[u8]) -> bool {
        let PageKey {
            program_id, page, ..
        } = PageKey::decode_all(&mut key).expect("Invalid key");
        self.actors
            .borrow()
            .get(&program_id)
            .and_then(|(actor, _)| actor.get_pages_data())
            .map(|pages_data| pages_data.contains_key(&page))
            .unwrap_or(false)
    }

    fn load_page(&mut self, mut key: &[u8], buffer: &mut [u8]) -> Option<u32> {
        let PageKey {
            program_id, page, ..
        } = PageKey::decode_all(&mut key).expect("Invalid key");
        let actors = self.actors.borrow();
        let (actor, _balance) = actors.get(&program_id)?;
        let pages_data = actor.get_pages_data()?;
        let page_buf = pages_data.get(&page)?;
        buffer.copy_from_slice(page_buf);
        Some(page_buf.len() as u32)
    }
}

/// The testing environment which simulates the chain state and its
/// transactions but somehow the real on-chain execution environment
/// could be different.
///
/// ```
/// use gtest::System;
///
/// // Create a new testing environment.
/// let system = System::new();
///
/// // Init logger with "gwasm" target set to `debug` level.
/// system.init_logger();
/// ```
pub struct System(pub(crate) RefCell<ExtManager>);

impl System {
    /// Prefix for lazy pages.
    pub(crate) const PAGE_STORAGE_PREFIX: [u8; 32] = *b"gtestgtestgtestgtestgtestgtest00";

    /// Create a new testing environment.
    ///
    /// # Panics
    /// Only one instance in the current thread of the `System` is possible to
    /// create. Instantiation of the other one leads to runtime panic.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        SYSTEM_INITIALIZED.with_borrow_mut(|initialized| {
            if *initialized {
                panic!("Impossible to have multiple instances of the `System`.");
            }

            let ext_manager = ExtManager::new();

            let actors = ext_manager.actors.clone();
            let pages_storage = PagesStorage { actors };
            gear_lazy_pages::init(
                LazyPagesVersion::Version1,
                LazyPagesInitContext::new(Self::PAGE_STORAGE_PREFIX),
                pages_storage,
            )
            .expect("Failed to init lazy-pages");

            *initialized = true;

            Self(RefCell::new(ext_manager))
        })
    }

    /// Init logger with "gwasm" target set to `debug` level.
    pub fn init_logger(&self) {
        self.init_logger_with_default_filter("gwasm=debug");
    }

    /// Init logger with "gwasm" and "gtest" targets set to `debug` level.
    pub fn init_verbose_logger(&self) {
        self.init_logger_with_default_filter("gwasm=debug,gtest=debug");
    }

    /// Init logger with `default_filter` as default filter.
    pub fn init_logger_with_default_filter<'a>(&self, default_filter: impl Into<Cow<'a, str>>) {
        let _ = Builder::from_env(Env::default().default_filter_or(default_filter))
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

    /// Send raw message dispatch.
    // todo [sab] remove that - it's a hack over the gtest and gear protocol.
    pub fn send_dispatch(&self, dispatch: Dispatch) -> RunResult {
        self.0.borrow_mut().validate_and_run_dispatch(dispatch)
    }

    // todo [sab] this must be spending blocks only on task pool
    /// Spend blocks and return all results.
    pub fn spend_blocks(&self, amount: u32) -> Vec<RunResult> {
        let mut manager = self.0.borrow_mut();
        let block_height = manager.blocks_manager.get().height;

        (block_height..block_height + amount)
            .map(|_| {
                manager.check_epoch();

                let block_info = manager.blocks_manager.next_block();
                let next_block_number = block_info.height;
                let mut results = manager.process_delayed_dispatches(next_block_number);
                results.extend(manager.process_scheduled_wait_list(next_block_number));
                results
            })
            .collect::<Vec<Vec<_>>>()
            .concat()
    }

    /// Return the current block height of the testing environment.
    pub fn block_height(&self) -> u32 {
        self.0.borrow().blocks_manager.get().height
    }

    /// Return the current block timestamp of the testing environment.
    pub fn block_timestamp(&self) -> u64 {
        self.0.borrow().blocks_manager.get().timestamp
    }

    /// Returns a [`Program`] by `id`.
    pub fn get_program<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> Option<Program> {
        let id = id.into().0;
        let manager = self.0.borrow();

        if manager.is_program(&id) {
            Some(Program {
                id,
                manager: &self.0,
            })
        } else {
            None
        }
    }

    /// Returns last added program.
    pub fn last_program(&self) -> Option<Program> {
        self.programs().into_iter().next_back()
    }

    /// Returns a list of programs.
    pub fn programs(&self) -> Vec<Program> {
        let manager = self.0.borrow();
        let actors = manager.actors.borrow();
        actors
            .keys()
            .copied()
            .filter(|id| manager.is_program(id))
            .map(|id| Program {
                id,
                manager: &self.0,
            })
            .collect()
    }

    /// Detect if a program is active with given `id`.
    ///
    /// An active program means that the program could be called,
    /// instead, if returns `false` it means that the program has
    /// exited or terminated that it can't be called anymore.
    pub fn is_active_program<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> bool {
        let program_id = id.into().0;
        self.0.borrow().is_active_program(&program_id)
    }

    /// Saves code to the storage and returns its code hash
    ///
    /// This method is mainly used for providing a proper program from program
    /// creation logic. In order to successfully create a new program with
    /// `gstd::prog::create_program_bytes_with_gas` function, developer should
    /// provide to the function "child's" code hash. Code for that code hash
    /// must be in storage at the time of the function call. So this method
    /// stores the code in storage.
    pub fn submit_code(&self, binary: impl Into<Vec<u8>>) -> CodeId {
        self.0.borrow_mut().store_new_code(binary.into())
    }

    /// Saves code from file to the storage and returns its code hash
    ///
    /// See also [`System::submit_code`]
    #[track_caller]
    pub fn submit_code_file<P: AsRef<Path>>(&self, code_path: P) -> CodeId {
        let code = fs::read(&code_path).unwrap_or_else(|_| {
            panic!(
                "Failed to read file {}",
                code_path.as_ref().to_string_lossy()
            )
        });
        self.0.borrow_mut().store_new_code(code)
    }

    /// Saves code to the storage and returns its code hash
    ///
    /// Same as ['submit_code_file'], but the path is provided as relative to
    /// the current directory.
    #[track_caller]
    pub fn submit_local_code_file<P: AsRef<Path>>(&self, code_path: P) -> CodeId {
        let path = env::current_dir()
            .expect("Unable to get root directory of the project")
            .join(code_path)
            .clean();

        self.submit_code_file(path)
    }

    /// Returns previously submitted code by its code hash.
    pub fn submitted_code(&self, code_id: CodeId) -> Option<Vec<u8>> {
        self.0.borrow().read_code(code_id).map(|code| code.to_vec())
    }

    /// Extract mailbox of user with given `id`.
    ///
    /// The mailbox contains messages from the program that are waiting
    /// for user action.
    #[track_caller]
    pub fn get_mailbox<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> ActorMailbox {
        let program_id = id.into().0;
        if !self.0.borrow().is_user(&program_id) {
            panic!("Mailbox available only for users");
        }
        ActorMailbox::new(program_id, &self.0)
    }

    /// Mint balance to user with given `id` and `value`.
    pub fn mint_to<ID: Into<ProgramIdWrapper>>(&self, id: ID, value: Balance) {
        let actor_id = id.into().0;
        self.0
            .borrow_mut()
            .mint_to(&actor_id, value, MintMode::KeepAlive);
    }

    /// Returns balance of user with given `id`.
    pub fn balance_of<ID: Into<ProgramIdWrapper>>(&self, id: ID) -> Balance {
        let actor_id = id.into().0;
        self.0.borrow().balance_of(&actor_id)
    }
}

impl Drop for System {
    fn drop(&mut self) {
        // Uninitialize
        SYSTEM_INITIALIZED.with_borrow_mut(|initialized| *initialized = false);
        self.0.borrow().gas_tree.reset();
        self.0.borrow().mailbox.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "Impossible to have multiple instances of the `System`.")]
    fn test_system_being_singleton() {
        let _first_instance = System::new();

        let _second_instance = System::new();
    }

    #[test]
    fn test_multithread_copy_singleton() {
        let first_instance = System::new();
        first_instance.spend_blocks(5);

        assert_eq!(first_instance.block_height(), 5);

        let h = std::thread::spawn(|| {
            let second_instance = System::new();

            second_instance.spend_blocks(10);
            assert_eq!(second_instance.block_height(), 10);
        });

        h.join().expect("internal error failed joining thread");
    }
}
