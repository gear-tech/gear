// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{Value, error::usage_panic};
use ethexe_common::{ProgramStates, Schedule, StateHashWithQueueSize, gear::MessageType};
use ethexe_runtime_common::state::{
    Dispatch as EthexeDispatch, MemStorage, ProgramState as EthexeProgramState, Storage,
};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    ids::{ActorId, CodeId, MessageId},
};
use std::collections::BTreeMap;

#[derive(Debug, Default)]
#[allow(dead_code)]
pub(crate) struct EthexeBackend {
    pub(crate) storage: MemStorage,
    pub(crate) states: ProgramStates,
    pub(crate) schedule: Schedule,
    pub(crate) code_ids: BTreeMap<ActorId, CodeId>,
    pub(crate) instrumented_codes: BTreeMap<CodeId, InstrumentedCode>,
    pub(crate) code_metadata: BTreeMap<CodeId, CodeMetadata>,
}

impl EthexeBackend {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn queue_len(&self) -> usize {
        self.states
            .values()
            .map(|state| {
                usize::from(state.canonical_queue_size)
                    .saturating_add(usize::from(state.injected_queue_size))
            })
            .sum()
    }

    pub(crate) fn register_program(
        &mut self,
        program_id: ActorId,
        code_id: CodeId,
        instrumented_code: InstrumentedCode,
        code_metadata: CodeMetadata,
    ) {
        let state = EthexeProgramState::zero();
        let hash = self.storage.write_program_state(state);

        self.states.insert(
            program_id,
            StateHashWithQueueSize {
                hash,
                canonical_queue_size: 0,
                injected_queue_size: 0,
            },
        );
        self.code_ids.insert(program_id, code_id);
        self.instrumented_codes.insert(code_id, instrumented_code);
        self.code_metadata.insert(code_id, code_metadata);
    }

    pub(crate) fn program_state(&self, program_id: ActorId) -> EthexeProgramState {
        let state = self
            .states
            .get(&program_id)
            .unwrap_or_else(|| panic!("ethexe program {program_id:?} is not registered"));

        self.storage
            .program_state(state.hash)
            .unwrap_or_else(|| panic!("ethexe state for {program_id:?} is missing"))
    }

    pub(crate) fn update_program_state(
        &mut self,
        program_id: ActorId,
        update: impl FnOnce(&mut EthexeProgramState, &MemStorage),
    ) {
        let mut state = self.program_state(program_id);
        update(&mut state, &self.storage);

        let canonical_queue_size = state.canonical_queue.cached_queue_size;
        let injected_queue_size = state.injected_queue.cached_queue_size;
        let hash = self.storage.write_program_state(state);

        self.states.insert(
            program_id,
            StateHashWithQueueSize {
                hash,
                canonical_queue_size,
                injected_queue_size,
            },
        );
    }

    pub(crate) fn queue_canonical(
        &mut self,
        program_id: ActorId,
        message_id: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: Value,
    ) {
        self.update_program_state(program_id, |state, storage| {
            let is_init = state.requires_init_message();
            let dispatch = EthexeDispatch::new(
                storage,
                message_id,
                source,
                payload,
                value,
                is_init,
                MessageType::Canonical,
                false,
            )
            .expect("failed to build canonical ethexe dispatch");

            state
                .canonical_queue
                .modify_queue(storage, |queue| queue.queue(dispatch));
        });
    }

    pub(crate) fn ensure_can_queue_injected(&self, program_id: ActorId) {
        if self.program_state(program_id).requires_init_message() {
            usage_panic!("Injected messages cannot be queued to an uninitialized ethexe program");
        }
    }

    pub(crate) fn queue_injected(
        &mut self,
        program_id: ActorId,
        message_id: MessageId,
        source: ActorId,
        payload: Vec<u8>,
        value: Value,
    ) {
        self.ensure_can_queue_injected(program_id);

        self.update_program_state(program_id, |state, storage| {
            let dispatch = EthexeDispatch::new(
                storage,
                message_id,
                source,
                payload,
                value,
                false,
                MessageType::Injected,
                false,
            )
            .expect("failed to build injected ethexe dispatch");

            state
                .injected_queue
                .modify_queue(storage, |queue| queue.queue(dispatch));
        });
    }

    pub(crate) fn top_up_executable_balance(&mut self, program_id: ActorId, value: Value) {
        self.update_program_state(program_id, |state, _| {
            state.executable_balance = state
                .executable_balance
                .checked_add(value)
                .expect("Overflow in executable_balance += value");
        });
    }

    pub(crate) fn top_up_balance(&mut self, program_id: ActorId, value: Value) {
        self.update_program_state(program_id, |state, _| {
            state.balance = state
                .balance
                .checked_add(value)
                .expect("Overflow in balance += value");
        });
    }

    pub(crate) fn balance_of(&self, program_id: ActorId) -> Value {
        self.program_state(program_id).balance
    }

    pub(crate) fn executable_balance_of(&self, program_id: ActorId) -> Value {
        self.program_state(program_id).executable_balance
    }
}
