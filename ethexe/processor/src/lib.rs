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
use ethexe_common::{
    mirror::RequestEvent as MirrorEvent, router::StateTransition, BlockRequestEvent,
};
use ethexe_db::{BlockMetaStorage, CodesStorage, Database};
use ethexe_runtime_common::state::Storage;
use gear_core::{
    ids::{prelude::CodeIdExt, ProgramId},
    message::ReplyInfo,
};
use gprimitives::{ActorId, CodeId, MessageId, H256};
use handling::run;
use host::InstanceCreator;
use std::collections::BTreeMap;

pub use common::LocalOutcome;

pub mod host;

mod common;
mod handling;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct Processor {
    db: Database,
    creator: InstanceCreator,
}

/// TODO: consider avoiding re-instantiations on processing events.
/// Maybe impl `struct EventProcessor`.
impl Processor {
    pub fn new(db: Database) -> Result<Self> {
        let creator = InstanceCreator::new(host::runtime())?;
        Ok(Self { db, creator })
    }

    pub fn overlaid(mut self) -> OverlaidProcessor {
        self.db = unsafe { self.db.overlaid() };

        OverlaidProcessor(self)
    }

    pub fn process_upload_code(
        &mut self,
        code_id: CodeId,
        code: &[u8],
    ) -> Result<Vec<LocalOutcome>> {
        log::debug!("Processing upload code {code_id:?}");

        let valid = code_id == CodeId::generate(code) && self.handle_new_code(code)?.is_some();

        self.db.set_code_valid(code_id, valid);
        Ok(vec![LocalOutcome::CodeValidated { id: code_id, valid }])
    }

    pub fn process_block_events(
        &mut self,
        block_hash: H256,
        events: Vec<BlockRequestEvent>,
    ) -> Result<Vec<LocalOutcome>> {
        log::debug!("Processing events for {block_hash:?}: {events:#?}");

        let mut states = self
            .db
            .block_start_program_states(block_hash)
            .unwrap_or_default(); // TODO (breathx): shouldn't it be a panic?

        let mut schedule = self.db.block_start_schedule(block_hash).unwrap_or_default(); // TODO (breathx): shouldn't it be a panic?

        let mut all_value_claims = Default::default();

        for event in events {
            match event {
                BlockRequestEvent::Router(event) => {
                    self.handle_router_event(&mut states, event)?;
                }
                BlockRequestEvent::Mirror { address, event } => {
                    self.handle_mirror_event(&mut states, &mut all_value_claims, address, event)?;
                }
                BlockRequestEvent::WVara(event) => {
                    self.handle_wvara_event(&mut states, event)?;
                }
            }
        }

        // TODO (breathx): handle outcomes.
        let mut _outcomes = self.run_tasks(block_hash, &mut states, &mut schedule)?;

        let mut outcomes = self.run(block_hash, &mut states)?;

        for outcome in &mut outcomes {
            if let LocalOutcome::Transition(StateTransition {
                actor_id,
                value_claims,
                ..
            }) = outcome
            {
                value_claims.extend(all_value_claims.remove(actor_id).unwrap_or_default());
            }
        }

        self.db.set_block_end_program_states(block_hash, states);
        self.db.set_block_end_schedule(block_hash, schedule);

        Ok(outcomes)
    }

    // TODO: replace LocalOutcome with Transition struct.
    pub fn run(
        &mut self,
        chain_head: H256,
        programs: &mut BTreeMap<ProgramId, H256>,
    ) -> Result<Vec<LocalOutcome>> {
        self.creator.set_chain_head(chain_head);

        log::debug!("{programs:?}");

        let messages_and_outcomes = run::run(8, self.db.clone(), self.creator.clone(), programs);

        Ok(messages_and_outcomes.1)
    }
}

#[derive(Clone)]
pub struct OverlaidProcessor(Processor);

impl OverlaidProcessor {
    // TODO (breathx): optimize for one single program.
    pub fn execute_for_reply(
        &mut self,
        block_hash: H256,
        source: ActorId,
        program_id: ActorId,
        payload: Vec<u8>,
        value: u128,
    ) -> Result<ReplyInfo> {
        self.0.creator.set_chain_head(block_hash);

        let mut states = self
            .0
            .db
            .block_start_program_states(block_hash)
            .unwrap_or_default();

        let mut value_claims = Default::default();

        let Some(&state_hash) = states.get(&program_id) else {
            return Err(anyhow::anyhow!("unknown program at specified block hash"));
        };

        let state =
            self.0.db.read_state(state_hash).ok_or_else(|| {
                anyhow::anyhow!("unreachable: state partially presents in storage")
            })?;

        anyhow::ensure!(
            !state.requires_init_message(),
            "program isn't yet initialized"
        );

        self.0.handle_mirror_event(
            &mut states,
            &mut value_claims,
            program_id,
            MirrorEvent::MessageQueueingRequested {
                id: MessageId::zero(),
                source,
                payload,
                value,
            },
        )?;

        let (messages, _) = run::run(8, self.0.db.clone(), self.0.creator.clone(), &mut states);

        let res = messages
            .into_iter()
            .find_map(|message| {
                message.reply_details().and_then(|details| {
                    (details.to_message_id() == MessageId::zero()).then(|| {
                        let parts = message.into_parts();

                        ReplyInfo {
                            payload: parts.3.into_vec(),
                            value: parts.5,
                            code: details.to_reply_code(),
                        }
                    })
                })
            })
            .ok_or_else(|| anyhow::anyhow!("reply wasn't found"))?;

        Ok(res)
    }
}
