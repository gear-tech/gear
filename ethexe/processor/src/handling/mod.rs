// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::Processor;
use anyhow::{anyhow, Result};
use ethexe_common::db::{BlockMetaStorageRead, CodesStorageWrite, OnChainStorageRead};
use ethexe_db::Database;
use ethexe_runtime_common::{
    state::ProgramState, InBlockTransitions, ScheduleHandler, TransitionController,
};
use gprimitives::{ActorId, CodeId, H256};

pub(crate) mod events;
pub(crate) mod run;

pub struct ProcessingHandler {
    pub block_hash: H256,
    pub db: Database,
    pub transitions: InBlockTransitions,
}

impl ProcessingHandler {
    pub fn controller(&mut self) -> TransitionController<'_, Database> {
        TransitionController {
            storage: &self.db,
            transitions: &mut self.transitions,
        }
    }

    pub fn update_state<T>(
        &mut self,
        program_id: ActorId,
        f: impl FnOnce(&mut ProgramState, &Database, &mut InBlockTransitions) -> T,
    ) -> T {
        self.controller().update_state(program_id, f)
    }
}

impl Processor {
    pub fn handler(&self, block_hash: H256) -> Result<ProcessingHandler> {
        let header = self
            .db
            .block_header(block_hash)
            .ok_or_else(|| anyhow!("failed to get block header for under-processing block"))?;

        let states = self
            .db
            .block_program_states(header.parent_hash)
            .ok_or_else(|| {
                anyhow!("failed to get block start program states for under-processing block")
            })?;

        let schedule = self.db.block_schedule(header.parent_hash).ok_or_else(|| {
            anyhow!("failed to get block start schedule for under-processing block")
        })?;

        let transitions = InBlockTransitions::new(header, states, schedule);

        Ok(ProcessingHandler {
            block_hash,
            db: self.db.clone(),
            transitions,
        })
    }

    /// Returns some CodeId in case of settlement and new code accepting.
    pub(crate) fn handle_new_code(
        &mut self,
        original_code: impl AsRef<[u8]>,
    ) -> Result<Option<CodeId>> {
        let mut executor = self.creator.instantiate()?;

        let original_code = original_code.as_ref();

        let Some(instrumented_code) = executor.instrument(original_code)? else {
            return Ok(None);
        };

        let code_id = self.db.set_original_code(original_code);

        self.db.set_instrumented_code(
            instrumented_code.instruction_weights_version(),
            code_id,
            instrumented_code,
        );

        Ok(Some(code_id))
    }
}

impl ProcessingHandler {
    pub fn run_schedule(&mut self) {
        let tasks = self.transitions.take_actual_tasks();

        log::debug!(
            "Running schedule for #{}: tasks are {tasks:?}",
            self.transitions.header().height
        );

        let mut handler = ScheduleHandler {
            controller: self.controller(),
        };

        for task in tasks {
            let _gas = task.process_with(&mut handler);
        }
    }
}
