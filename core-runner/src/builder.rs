// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use super::runner::{Config, ExtMessage, InitializeProgramInfo, Runner};
use crate::ext::Ext;
use alloc::vec::*;
use gear_backend_common::Environment;
use gear_core::storage::{MessageQueue, ProgramStorage, Storage, WaitList};

#[cfg(test)]
use gear_core::storage::{InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList};

/// Builder for [`Runner`].
#[derive(Default)]
pub struct RunnerBuilder<MQ: MessageQueue, PS: ProgramStorage, WL: WaitList, E: Environment<Ext>> {
    config: Config,
    programs: Vec<InitializeProgramInfo>,
    storage: Storage<MQ, PS, WL>,
    block_height: u32,
    env: core::marker::PhantomData<E>,
}

#[cfg(test)]
/// Fully in-memory runner builder (for tests).
pub type InMemoryRunnerBuilder<E> =
    RunnerBuilder<InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList, E>;

impl<MQ: MessageQueue, PS: ProgramStorage, WL: WaitList, E: Environment<Ext>>
    RunnerBuilder<MQ, PS, WL, E>
{
    /// Create an empty `RunnerBuilder` for default [`Runner`].
    pub fn new() -> Self {
        Default::default()
    }

    /// Set block height.
    pub fn block_height(mut self, value: u32) -> Self {
        self.block_height = value;
        self
    }

    /// Set [`Config`].
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    /// Add a program code to be initialized on build.
    pub fn program(mut self, code: Vec<u8>) -> Self {
        let counter = self.programs.len() as u64;

        self.programs.push(InitializeProgramInfo {
            source_id: 1001.into(),
            new_program_id: (1 + counter).into(),
            code,
            message: ExtMessage {
                id: (1000000 + counter).into(),
                payload: Vec::new(),
                gas_limit: u64::MAX,
                value: 0,
            },
        });
        self
    }

    /// Change the source ID in the last added program info.
    pub fn with_source_id(mut self, id: u64) -> Self {
        let program = self
            .programs
            .last_mut()
            .expect("No any program added, call `program()` before");
        program.source_id = id.into();
        self
    }

    /// Change the program ID of the last added program.
    pub fn with_program_id(mut self, id: u64) -> Self {
        let program = self
            .programs
            .last_mut()
            .expect("No any program added, call `program()` before");
        program.new_program_id = id.into();
        self
    }

    /// Change the init message in the last added program info.
    pub fn with_init_message(mut self, message: ExtMessage) -> Self {
        let program = self
            .programs
            .last_mut()
            .expect("No any program added, call `program()` before");
        program.message = message;
        self
    }

    /// Initialize all programs and return [`Runner`].
    pub fn build(self) -> Runner<MQ, PS, WL, E> {
        let mut runner = Runner::new(&self.config, self.storage, self.block_height, E::default());
        for program in self.programs {
            runner
                .init_program(program)
                .expect("failed to init program");
        }
        runner
    }
}
