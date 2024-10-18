// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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
use anyhow::Result;
use ethexe_db::CodesStorage;
use ethexe_runtime_common::{
    state::{ComplexStorage as _, Dispatch},
    InBlockTransitions, ScheduleHandler,
};
use gprimitives::{CodeId, H256};

pub(crate) mod events;
pub(crate) mod run;

impl Processor {
    pub fn run_schedule(&mut self, in_block_transitions: &mut InBlockTransitions) {
        let tasks = in_block_transitions.take_actual_tasks();

        log::debug!(
            "Running schedule for #{}: tasks are {tasks:?}",
            in_block_transitions.header().height
        );

        let mut handler = ScheduleHandler {
            in_block_transitions,
            storage: &self.db,
        };

        for task in tasks {
            let _gas = task.process_with(&mut handler);
        }
    }

    pub(crate) fn handle_message_queueing(
        &mut self,
        state_hash: H256,
        dispatch: Dispatch,
    ) -> Result<H256> {
        self.handle_messages_queueing(state_hash, vec![dispatch])
    }

    pub(crate) fn handle_messages_queueing(
        &mut self,
        state_hash: H256,
        dispatches: Vec<Dispatch>,
    ) -> Result<H256> {
        if dispatches.is_empty() {
            return Ok(state_hash);
        }

        self.db.mutate_state(state_hash, |processor, state| {
            anyhow::ensure!(state.program.is_active(), "program should be active");

            state.queue_hash = processor
                .modify_queue(state.queue_hash.clone(), |queue| queue.extend(dispatches))?;

            Ok(())
        })
    }

    /// Returns some CodeId in case of settlement and new code accepting.
    pub(crate) fn handle_new_code(
        &mut self,
        original_code: impl AsRef<[u8]>,
    ) -> Result<Option<CodeId>> {
        let mut executor = self.creator.instantiate()?;

        let original_code = original_code.as_ref();

        let Some((instrumented_code, code_metadata)) = executor.instrument(original_code)? else {
            return Ok(None);
        };

        let code_id = self.db.set_original_code(original_code);

        self.db.set_instrumented_code(
            code_metadata.instruction_weights_version(),
            code_id,
            instrumented_code,
        );

        Ok(Some(code_id))
    }
}
