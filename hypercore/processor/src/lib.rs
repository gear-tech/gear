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

use anyhow::Result;
use core_processor::common::JournalNote;
use gear_core::{
    ids::{prelude::CodeIdExt, ActorId, MessageId, ProgramId},
    message::DispatchKind,
};
use gprimitives::{CodeId, H256};
use host::InstanceCreator;
use hypercore_db::Database;
use hypercore_observer::Event;
use parity_scale_codec::{Decode, Encode};
use std::collections::BTreeMap;

pub mod host;
mod run;

#[cfg(test)]
mod tests;

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

    // TODO: use proper `Dispatch` type here instead of db's.
    pub fn run(
        &mut self,
        programs: BTreeMap<ProgramId, H256>,
        messages: BTreeMap<ProgramId, Vec<UserMessage>>,
    ) -> Result<()> {
        let mut programs = programs;
        let _messages_to_users = run::run(8, self.creator.clone(), &mut programs, messages);
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
                parent_hash: _,
                block_number: _,
                timestamp: _,
                events: _,
            } => {
                log::debug!("Processing events for {block_hash:?}");
                Ok(vec![])
            }
        }
    }
}
