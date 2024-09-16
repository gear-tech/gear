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
    mirror::RequestEvent as MirrorEvent,
    router::{RequestEvent as RouterEvent, StateTransition},
    wvara::RequestEvent as WVaraEvent,
    BlockRequestEvent,
};
use ethexe_db::{BlockMetaStorage, CodesStorage, Database};
use ethexe_runtime_common::state::{Dispatch, HashAndLen, MaybeHash, Storage};
use gear_core::{
    ids::{prelude::CodeIdExt, ProgramId},
    message::{DispatchKind, Payload},
};
use gprimitives::{ActorId, CodeId, H256};
use host::InstanceCreator;
use parity_scale_codec::{Decode, Encode};
use std::collections::{BTreeMap, VecDeque};

pub mod host;
mod run;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct Processor {
    db: Database,
    creator: InstanceCreator,
}

#[derive(Clone)]
pub struct OverlaidProcessor(Processor);

impl OverlaidProcessor {
    pub fn execute_for_reply(&mut self, block_hash: H256, _program_id: ActorId) -> Result<Vec<u8>> {
        self.0.creator.set_chain_head(block_hash);
        Ok(Default::default())
    }
}

/// Local changes that can be committed to the network or local signer.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub enum LocalOutcome {
    /// Produced when code with specific id is recorded and validated.
    CodeValidated {
        id: CodeId,
        valid: bool,
    },

    Transition(StateTransition),
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

    /// Returns some CodeId in case of settlement and new code accepting.
    pub fn handle_new_code(&mut self, original_code: impl AsRef<[u8]>) -> Result<Option<CodeId>> {
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

    /// Returns bool defining was newly re-instrumented code settled or not.
    pub fn reinstrument_code(&mut self, code_id: CodeId) -> Result<bool> {
        let Some(original_code) = self.db.original_code(code_id) else {
            anyhow::bail!("it's impossible to reinstrument inexistent code");
        };

        let mut executor = self.creator.instantiate()?;

        let Some(instrumented_code) = executor.instrument(&original_code)? else {
            return Ok(false);
        };

        self.db.set_instrumented_code(
            instrumented_code.instruction_weights_version(),
            code_id,
            instrumented_code,
        );

        Ok(true)
    }

    pub fn handle_new_program(&mut self, program_id: ProgramId, code_id: CodeId) -> Result<()> {
        if self.db.original_code(code_id).is_none() {
            anyhow::bail!("code existence should be checked on smart contract side");
        }

        if self.db.program_code_id(program_id).is_some() {
            anyhow::bail!("program duplicates should be checked on smart contract side");
        }

        self.db.set_program_code_id(program_id, code_id);

        Ok(())
    }

    pub fn handle_executable_balance_top_up(
        &mut self,
        state_hash: H256,
        value: u128,
    ) -> Result<H256> {
        let mut state = self
            .db
            .read_state(state_hash)
            .ok_or_else(|| anyhow::anyhow!("program should exist"))?;

        state.executable_balance += value;

        Ok(self.db.write_state(state))
    }

    pub fn handle_payload(&mut self, payload: Vec<u8>) -> Result<MaybeHash> {
        let payload = Payload::try_from(payload)
            .map_err(|_| anyhow::anyhow!("payload should be checked on eth side"))?;

        let hash = payload
            .inner()
            .is_empty()
            .then_some(MaybeHash::Empty)
            .unwrap_or_else(|| self.db.write_payload(payload).into());

        Ok(hash)
    }

    pub fn handle_message_queueing(
        &mut self,
        state_hash: H256,
        dispatch: Dispatch,
    ) -> Result<H256> {
        self.handle_messages_queueing(state_hash, vec![dispatch])
    }

    pub fn handle_messages_queueing(
        &mut self,
        state_hash: H256,
        dispatches: Vec<Dispatch>,
    ) -> Result<H256> {
        if dispatches.is_empty() {
            return Ok(state_hash);
        }

        let mut state = self
            .db
            .read_state(state_hash)
            .ok_or_else(|| anyhow::anyhow!("program should exist"))?;

        anyhow::ensure!(state.program.is_active(), "program should be active");

        let queue = if let MaybeHash::Hash(HashAndLen {
            hash: queue_hash, ..
        }) = state.queue_hash
        {
            let mut queue = self
                .db
                .read_queue(queue_hash)
                .ok_or_else(|| anyhow::anyhow!("queue should exist if hash present"))?;

            queue.extend(dispatches);

            queue
        } else {
            VecDeque::from(dispatches)
        };

        state.queue_hash = self.db.write_queue(queue).into();

        Ok(self.db.write_state(state))
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
            .unwrap_or_default();

        for event in events {
            match event {
                BlockRequestEvent::Router(event) => {
                    self.handle_router_event(&mut states, event)?;
                }
                BlockRequestEvent::Mirror { address, event } => {
                    self.handle_mirror_event(&mut states, address, event)?;
                }
                BlockRequestEvent::WVara(event) => {
                    self.handle_wvara_event(&mut states, event)?;
                }
            }
        }

        let outcomes = self.run(block_hash, &mut states)?;

        self.db.set_block_end_program_states(block_hash, states);

        Ok(outcomes)
    }

    fn handle_router_event(
        &mut self,
        states: &mut BTreeMap<ProgramId, H256>,
        event: RouterEvent,
    ) -> Result<()> {
        match event {
            RouterEvent::ProgramCreated { actor_id, code_id } => {
                self.handle_new_program(actor_id, code_id)?;

                states.insert(actor_id, H256::zero());
            }
            RouterEvent::CodeValidationRequested { .. }
            | RouterEvent::BaseWeightChanged { .. }
            | RouterEvent::StorageSlotChanged
            | RouterEvent::ValidatorsSetChanged
            | RouterEvent::ValuePerWeightChanged { .. } => {
                log::debug!("Handler not yet implemented: {event:?}");
                return Ok(());
            }
        };

        Ok(())
    }

    fn handle_mirror_event(
        &mut self,
        states: &mut BTreeMap<ProgramId, H256>,
        actor_id: ProgramId,
        event: MirrorEvent,
    ) -> Result<()> {
        let Some(&state_hash) = states.get(&actor_id) else {
            log::debug!("Received event from unrecognized mirror ({actor_id}): {event:?}");

            return Ok(());
        };

        let new_state_hash = match event {
            MirrorEvent::ExecutableBalanceTopUpRequested { value } => {
                self.handle_executable_balance_top_up(state_hash, value)?
            }
            MirrorEvent::MessageQueueingRequested {
                id,
                source,
                payload,
                value,
            } => {
                let payload_hash = self.handle_payload(payload)?;

                let state = self
                    .db
                    .read_state(state_hash)
                    .ok_or_else(|| anyhow::anyhow!("program should exist"))?;

                let kind = if state.requires_init_message() {
                    DispatchKind::Init
                } else {
                    DispatchKind::Handle
                };

                let dispatch = Dispatch {
                    id,
                    kind,
                    source,
                    payload_hash,
                    value,
                    details: None,
                    context: None,
                };

                self.handle_message_queueing(state_hash, dispatch)?
            }
            MirrorEvent::ReplyQueueingRequested { .. }
            | MirrorEvent::ValueClaimingRequested { .. } => {
                log::debug!("Handler not yet implemented: {event:?}");
                return Ok(());
            }
        };

        states.insert(actor_id, new_state_hash);

        Ok(())
    }

    fn handle_wvara_event(
        &mut self,
        _states: &mut BTreeMap<ProgramId, H256>,
        event: WVaraEvent,
    ) -> Result<()> {
        match event {
            WVaraEvent::Transfer { .. } => {
                log::debug!("Handler not yet implemented: {event:?}");
                Ok(())
            }
        }
    }
}
