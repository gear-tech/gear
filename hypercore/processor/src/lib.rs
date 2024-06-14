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

//! Program's execution service for eGPU.

use std::collections::VecDeque;

use anyhow::Result;
use core_processor::common::JournalNote;
use gear_core::{
    ids::{prelude::CodeIdExt, ActorId, MessageId, ProgramId},
    message::{DispatchKind, Payload},
    program::MemoryInfix,
};
use gprimitives::{CodeId, H256};
use host::InstanceCreator;
use hypercore_db::Database;
use hypercore_observer::Event;
use hypercore_runtime_common::state::{
    self, ActiveProgram, Dispatch, MaybeHash, ProgramState, Storage,
};
use parity_scale_codec::{Decode, Encode};

pub mod host;
mod run;

#[cfg(test)]
mod tests;

#[allow(unused)]
pub struct UserMessage {
    id: MessageId,
    kind: DispatchKind,
    source: ActorId,
    payload: Vec<u8>,
    gas_limit: u64,
    value: u128,
}

pub struct Processor {
    db: Database,
    creator: InstanceCreator,
}

/// Local changes that can be committed to the network or local signer.
#[derive(Debug, Encode, Decode)]
pub enum LocalOutcome {
    /// Produced when code with specific id is recorded and available in database.
    CodeApproved(CodeId),

    // TODO: add docs
    CodeRejected(CodeId),
}

/// TODO: consider avoiding re-instantiations on processing events.
/// Maybe impl `struct EventProcessor`.
impl Processor {
    pub fn new(db: Database) -> Result<Self> {
        let creator = InstanceCreator::new(db.clone(), host::runtime())?;
        Ok(Self { db, creator })
    }

    /// Returns some CodeId in case of settlement and new code accepting.
    pub fn handle_new_code(&mut self, original_code: impl AsRef<[u8]>) -> Result<Option<CodeId>> {
        let mut executor = self.creator.instantiate()?;

        let original_code = original_code.as_ref();

        let Some(instrumented_code) = executor.instrument(original_code)? else {
            return Ok(None);
        };

        let code_id = self.db.write_original_code(original_code);

        self.db.write_instrumented_code(
            instrumented_code.instruction_weights_version(),
            code_id,
            instrumented_code,
        );

        Ok(Some(code_id))
    }

    /// Returns bool defining was newly re-instrumented code settled or not.
    pub fn reinstrument_code(&mut self, code_id: CodeId) -> Result<bool> {
        let Some(original_code) = self.db.read_original_code(code_id) else {
            anyhow::bail!("it's impossible to reinstrument inexistent code");
        };

        let mut executor = self.creator.instantiate()?;

        let Some(instrumented_code) = executor.instrument(&original_code)? else {
            return Ok(false);
        };

        self.db.write_instrumented_code(
            instrumented_code.instruction_weights_version(),
            code_id,
            instrumented_code,
        );

        Ok(true)
    }

    // TODO: deal with params on smart contract side.
    pub fn handle_new_program(&mut self, program_id: ProgramId, code_id: CodeId) -> Result<H256> {
        if self.db.read_original_code(code_id).is_none() {
            anyhow::bail!("code existence should be checked on smart contract side");
        }

        if self.db.get_program_code_id(program_id).is_some() {
            anyhow::bail!("program duplicates should be checked on smart contract side");
        }

        self.db.set_program_code_id(program_id, code_id);

        // TODO: state here is non-zero (?!).

        let active_program = ActiveProgram {
            allocations_hash: MaybeHash::Empty,
            pages_hash: MaybeHash::Empty,
            gas_reservation_map_hash: MaybeHash::Empty,
            memory_infix: MemoryInfix::new(0),
            initialized: false,
        };

        // TODO: on program creation send message to it.
        let program_state = ProgramState {
            state: state::Program::Active(active_program),
            queue_hash: MaybeHash::Empty,
            waitlist_hash: MaybeHash::Empty,
            // TODO: remove program balance from here.
            balance: 0,
        };

        // TODO: not write zero state, but just register it (or support default on get)
        Ok(self.db.write_state(program_state))
    }

    // TODO: remove state hashes from here
    pub fn handle_user_message(
        &mut self,
        program_hash: H256,
        messages: Vec<UserMessage>,
    ) -> Result<H256> {
        if messages.is_empty() {
            return Ok(program_hash);
        }

        let mut dispatches = Vec::with_capacity(messages.len());

        for message in messages {
            let payload = Payload::try_from(message.payload)
                .map_err(|_| anyhow::anyhow!("payload should be checked on eth side"))?;

            let payload_hash = payload
                .inner()
                .is_empty()
                .then_some(MaybeHash::Empty)
                .unwrap_or_else(|| self.db.write_payload(payload).into());

            let dispatch = Dispatch {
                id: message.id,
                kind: message.kind,
                source: message.source,
                payload_hash,
                gas_limit: message.gas_limit,
                value: message.value,
                // TODO: handle replies.
                details: None,
                context: None,
            };

            dispatches.push(dispatch);
        }

        // TODO: on zero hash return default avoiding db.
        let mut program_state = self
            .db
            .read_state(program_hash)
            .ok_or_else(|| anyhow::anyhow!("program should exist"))?;

        let mut queue = if let MaybeHash::Hash(queue_hash_and_len) = program_state.queue_hash {
            self.db
                .read_queue(queue_hash_and_len.hash)
                .ok_or_else(|| anyhow::anyhow!("queue should exist if hash present"))?
        } else {
            VecDeque::with_capacity(dispatches.len())
        };

        queue.extend(dispatches);

        let queue_hash = self.db.write_queue(queue);

        program_state.queue_hash = MaybeHash::Hash(queue_hash.into());

        Ok(self.db.write_state(program_state))
    }

    pub fn run_on_host(
        &mut self,
        program_id: ProgramId,
        program_state: H256,
    ) -> Result<Vec<JournalNote>> {
        let original_code_id = self.db.get_program_code_id(program_id).unwrap();

        let maybe_instrumented_code = self
            .db
            .read_instrumented_code(hypercore_runtime::VERSION, original_code_id);

        let mut executor = self.creator.instantiate()?;

        executor.run(
            program_id,
            original_code_id,
            program_state,
            maybe_instrumented_code,
        )
    }

    pub fn run(&mut self, chain_head: H256) -> Result<()> {
        self.creator.set_chain_head(chain_head);

        let mut programs = self
            .db
            .get_block_program_hashes(chain_head)
            .expect("Programs map is not found");

        let _messages_to_users = run::run(8, self.creator.clone(), &mut programs);

        Ok(())
    }

    pub fn process_observer_event(&mut self, event: &Event) -> Result<Vec<LocalOutcome>> {
        match event {
            Event::UploadCode { code_id, code, .. } => {
                log::debug!("Processing upload code {code_id:?}");

                if *code_id != CodeId::generate(code) || self.handle_new_code(code)?.is_none() {
                    Ok(vec![LocalOutcome::CodeRejected(*code_id)])
                } else {
                    Ok(vec![LocalOutcome::CodeApproved(*code_id)])
                }
            }
            Event::Block {
                ref block_hash,
                events: _,
            } => {
                log::debug!("Processing events for {block_hash:?}");
                Ok(vec![])
            }
        }
    }
}
