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
use common::{
    scheduler::{ScheduledTask, TaskHandler},
    CodeId, Gas, MessageId, ProgramId, ReservationId,
};
use ethexe_db::BlockMetaStorage;
use gear_core::message::Message;
use gprimitives::{ActorId, H256};
use std::collections::BTreeMap;

impl Processor {
    pub fn run_tasks(
        &mut self,
        block_hash: H256,
        states: &mut BTreeMap<ProgramId, H256>,
        tasks: &mut BTreeMap<u32, Vec<ScheduledTask<ActorId>>>,
    ) -> Result<Vec<LocalOutcome>> {
        let mut handler = TasksHandler {
            states,
            results: Default::default(),
            to_users_messages: Default::default(),
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
    pub states: &'a mut BTreeMap<ProgramId, H256>,
    pub results: BTreeMap<ActorId, H256>,
    pub to_users_messages: Vec<Message>,
}

impl<'a> TaskHandler<ActorId> for TasksHandler<'a> {
    fn remove_from_mailbox(&mut self, _user_id: ActorId, _message_id: MessageId) -> Gas {
        unimplemented!("TODO (breathx)")
    }
    fn remove_from_waitlist(&mut self, _program_id: ProgramId, _message_id: MessageId) -> Gas {
        unimplemented!("TODO (breathx)")
    }
    fn send_dispatch(&mut self, _stashed_message_id: MessageId) -> Gas {
        unimplemented!("TODO (breathx)")
    }
    fn send_user_message(&mut self, _stashed_message_id: MessageId, _to_mailbox: bool) -> Gas {
        unimplemented!("TODO (breathx)")
    }
    fn wake_message(&mut self, _program_id: ProgramId, _message_id: MessageId) -> Gas {
        // TODO (breathx): consider deprecation of delayed wakes + non-concrete waits.
        unimplemented!("TODO (breathx)")
    }

    /* Deprecated APIs */
    fn pause_program(&mut self, _: ProgramId) -> Gas {
        unreachable!("deprecated")
    }
    fn remove_code(&mut self, _: CodeId) -> Gas {
        unreachable!("deprecated")
    }
    fn remove_gas_reservation(&mut self, _: ProgramId, _: ReservationId) -> Gas {
        unreachable!("deprecated")
    }
    fn remove_paused_program(&mut self, _: ProgramId) -> Gas {
        unreachable!("deprecated")
    }
    fn remove_resume_session(&mut self, _: u32) -> Gas {
        unreachable!("deprecated")
    }
}
