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

use anyhow::{anyhow, ensure, Result};
use ethexe_common::{mirror::RequestEvent as MirrorEvent, BlockRequestEvent};
use ethexe_db::{BlockMetaStorage, CodesStorage, Database};
use ethexe_runtime_common::{state::Storage, InBlockTransitions};
use gear_core::{ids::prelude::CodeIdExt, message::ReplyInfo};
use gprimitives::{ActorId, CodeId, MessageId, H256};
use handling::run;
use host::InstanceCreator;

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

        let mut handler = self.handler(block_hash)?;

        for event in events {
            match event {
                BlockRequestEvent::Router(event) => {
                    handler.handle_router_event(event)?;
                }
                BlockRequestEvent::Mirror { address, event } => {
                    handler.handle_mirror_event(address, event)?;
                }
                BlockRequestEvent::WVara(event) => {
                    handler.handle_wvara_event(event);
                }
            }
        }

        handler.run_schedule();
        self.run(block_hash, &mut handler.transitions);

        let (transitions, states, schedule) = handler.transitions.finalize();

        self.db.set_block_end_program_states(block_hash, states);
        self.db.set_block_end_schedule(block_hash, schedule);

        let outcomes = transitions
            .into_iter()
            .map(LocalOutcome::Transition)
            .collect();

        Ok(outcomes)
    }

    pub fn run(&mut self, chain_head: H256, in_block_transitions: &mut InBlockTransitions) {
        self.creator.set_chain_head(chain_head);

        run::run(
            8,
            self.db.clone(),
            self.creator.clone(),
            in_block_transitions,
        );
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

        let mut handler = self.0.handler(block_hash)?;

        let state_hash = handler
            .transitions
            .state_of(&program_id)
            .ok_or_else(|| anyhow!("unknown program at specified block hash"))?;

        let state = handler
            .db
            .read_state(state_hash)
            .ok_or_else(|| anyhow!("unreachable: state partially presents in storage"))?;

        ensure!(
            !state.requires_init_message(),
            "program isn't yet initialized"
        );

        handler.handle_mirror_event(
            program_id,
            MirrorEvent::MessageQueueingRequested {
                id: MessageId::zero(),
                source,
                payload,
                value,
            },
        )?;

        run::run(
            8,
            self.0.db.clone(),
            self.0.creator.clone(),
            &mut handler.transitions,
        );

        let res = handler
            .transitions
            .current_messages()
            .into_iter()
            .find_map(|(_source, message)| {
                message.reply_details.and_then(|details| {
                    (details.to_message_id() == MessageId::zero()).then(|| ReplyInfo {
                        payload: message.payload,
                        value: message.value,
                        code: details.to_reply_code(),
                    })
                })
            })
            .ok_or_else(|| anyhow!("reply wasn't found"))?;

        Ok(res)
    }
}
