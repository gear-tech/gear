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

use crate::{LocalOutcome, Processor};
use anyhow::Result;
use ethexe_db::BlockMetaStorage;
use ethexe_runtime_common::InBlockTransitions;
use gear_core::{
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    tasks::{ScheduledTask, TaskHandler},
};
use gprimitives::{ActorId, H256};
use std::collections::BTreeMap;

impl Processor {
    pub fn run_tasks(
        &mut self,
        block_hash: H256,
        in_block_transitions: &mut InBlockTransitions,
        tasks: &mut BTreeMap<u32, Vec<ScheduledTask<ActorId>>>,
    ) -> Result<Vec<LocalOutcome>> {
        let mut handler = TasksHandler {
            in_block_transitions,
        };

        let block_meta = self
            .db
            .block_header(block_hash)
            .ok_or_else(|| anyhow::anyhow!("block header for chain head wasn't found"))?;

        let tasks = tasks.remove(&block_meta.height).unwrap_or_default();

        for task in tasks {
            let _gas = task.process_with(&mut handler);
        }

        Ok(vec![])
    }
}

#[allow(unused)]
pub struct TasksHandler<'a> {
    pub in_block_transitions: &'a mut InBlockTransitions,
}

impl<'a> TaskHandler<ActorId> for TasksHandler<'a> {
    fn remove_from_mailbox(&mut self, _user_id: ActorId, _message_id: MessageId) -> u64 {
        unimplemented!("TODO (breathx)")
    }
    fn remove_from_waitlist(&mut self, _program_id: ProgramId, _message_id: MessageId) -> u64 {
        unimplemented!("TODO (breathx)")
    }
    fn send_dispatch(&mut self, _stashed_message_id: MessageId) -> u64 {
        unimplemented!("TODO (breathx)")
    }
    fn send_user_message(&mut self, _stashed_message_id: MessageId, _to_mailbox: bool) -> u64 {
        unimplemented!("TODO (breathx)")
    }
    fn wake_message(&mut self, _program_id: ProgramId, _message_id: MessageId) -> u64 {
        // TODO (breathx): consider deprecation of delayed wakes + non-concrete waits.
        unimplemented!("TODO (breathx)")
    }

    /* Deprecated APIs */
    fn pause_program(&mut self, _: ProgramId) -> u64 {
        unreachable!("deprecated")
    }
    fn remove_code(&mut self, _: CodeId) -> u64 {
        unreachable!("deprecated")
    }
    fn remove_gas_reservation(&mut self, _: ProgramId, _: ReservationId) -> u64 {
        unreachable!("deprecated")
    }
    fn remove_paused_program(&mut self, _: ProgramId) -> u64 {
        unreachable!("deprecated")
    }
    fn remove_resume_session(&mut self, _: u32) -> u64 {
        unreachable!("deprecated")
    }
}
