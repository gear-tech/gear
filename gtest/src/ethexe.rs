// This file is part of Gear.

// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{Gas, Value, error::usage_panic, log::BlockRunResult};
use core_processor::configs::BlockInfo;
use ethexe_common::{
    CodeAndIdUnchecked, ProgramStates, Schedule, SimpleBlockData, StateHashWithQueueSize,
    db::{CodesStorageRO, CodesStorageRW},
    ecdsa::VerifiedData,
    events::{BlockRequestEvent, MirrorRequestEvent, mirror::MessageQueueingRequestedEvent},
    gear::StateTransition,
    injected::InjectedTransaction,
};
use ethexe_db::Database;
use ethexe_processor::{ExecutableData, ProcessedCodeInfo, Processor, ValidCodeInfo};
use ethexe_runtime_common::{
    RUNTIME_ID,
    state::{ProgramState, Storage},
};
use gear_core::{
    ids::{ActorId, CodeId, MessageId, prelude::MessageIdExt as _},
    message::ReplyCode,
    rpc::ReplyInfo,
};
use gsigner::secp256k1::Secp256k1SignerExt as _;
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
};

fn block_on<F, T>(future: F) -> T
where
    F: Future<Output = T> + Send,
    T: Send,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("failed to build ethexe gtest runtime")
                        .block_on(future)
                })
                .join()
                .expect("ethexe gtest runtime thread panicked")
        })
    } else {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build ethexe gtest runtime")
            .block_on(future)
    }
}

pub(crate) struct EthexeManager {
    db: Database,
    processor: Processor,
    program_states: ProgramStates,
    schedule: Schedule,
    pending_events: Vec<BlockRequestEvent>,
    pending_injected: Vec<VerifiedData<InjectedTransaction>>,
    block: SimpleBlockData,
    message_nonce: u64,
    last_executable_balance_burned: Value,
}

impl EthexeManager {
    pub(crate) fn new() -> Self {
        // gtest owns this in-memory database for the lifetime of one System.
        let db = unsafe { Database::memory() };
        let processor = Processor::new(db.clone()).expect("failed to create ethexe processor");

        log::debug!(target: "gtest::ethexe", "Initialized ethexe gtest manager");

        Self {
            db,
            processor,
            program_states: Default::default(),
            schedule: Default::default(),
            pending_events: Default::default(),
            pending_injected: Default::default(),
            block: Default::default(),
            message_nonce: 0,
            last_executable_balance_burned: 0,
        }
    }

    pub(crate) fn queue_len(&self) -> usize {
        let queue_len = self
            .program_states
            .values()
            .map(|state| state.canonical_queue_size as usize + state.injected_queue_size as usize)
            .sum::<usize>()
            + self
                .pending_events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        BlockRequestEvent::Mirror {
                            event: MirrorRequestEvent::MessageQueueingRequested(_),
                            ..
                        }
                    )
                })
                .count()
            + self.pending_injected.len();

        log::trace!(target: "gtest::ethexe", "Ethexe queue length requested: {queue_len}");

        queue_len
    }

    pub(crate) fn program_ids(&self) -> Vec<ActorId> {
        self.program_states.keys().copied().collect()
    }

    pub(crate) fn block_height(&self) -> u32 {
        self.block.header.height
    }

    pub(crate) fn block_timestamp(&self) -> u64 {
        self.block.header.timestamp
    }

    pub(crate) fn store_code(&mut self, code_id: CodeId, code: Vec<u8>) {
        let original_len = code.len();
        log::debug!(
            target: "gtest::ethexe",
            "Processing ethexe code {code_id} ({original_len} bytes)"
        );

        let ProcessedCodeInfo { valid, .. } =
            block_on(self.processor.process_code(CodeAndIdUnchecked {
                code: code.clone(),
                code_id,
            }))
            .expect("failed to process ethexe code");

        let ValidCodeInfo {
            code,
            instrumented_code,
            code_metadata,
        } = valid.expect("provided ethexe code is invalid");
        let instrumented_len = instrumented_code.bytes().len();

        self.db.set_original_code(&code);
        self.db
            .set_instrumented_code(RUNTIME_ID, code_id, instrumented_code);
        self.db.set_code_metadata(code_id, code_metadata);
        self.db.set_code_valid(code_id, true);

        log::debug!(
            target: "gtest::ethexe",
            "Stored ethexe code {code_id}: original={original_len} bytes, instrumented={} bytes",
            instrumented_len
        );
    }

    pub(crate) fn original_code(&self, code_id: CodeId) -> Option<Vec<u8>> {
        self.db.original_code(code_id)
    }

    pub(crate) fn register_program(&mut self, actor_id: ActorId, code_id: CodeId) {
        if self.is_program(actor_id) {
            usage_panic!(
                "Can't create program with id {actor_id}, because Program with this id already exists. \
                Please, use another id."
            );
        }

        self.db.set_program_code_id(actor_id, code_id);
        let state = ProgramState::zero();
        let state_hash = self.db.write_program_state(state);
        self.program_states.insert(
            actor_id,
            StateHashWithQueueSize {
                hash: state_hash,
                canonical_queue_size: 0,
                injected_queue_size: 0,
            },
        );

        log::debug!(
            target: "gtest::ethexe",
            "Registered ethexe program {actor_id} with code {code_id}"
        );
    }

    pub(crate) fn is_program(&self, actor_id: ActorId) -> bool {
        self.program_states.contains_key(&actor_id)
    }

    pub(crate) fn state(&self, actor_id: ActorId) -> ProgramState {
        let state = self
            .program_states
            .get(&actor_id)
            .unwrap_or_else(|| panic!("ethexe program {actor_id} not found"));
        self.db
            .program_state(state.hash)
            .expect("ethexe program state missing from database")
    }

    pub(crate) fn balance_of(&self, actor_id: ActorId) -> Value {
        self.state(actor_id).balance
    }

    pub(crate) fn executable_balance_of(&self, actor_id: ActorId) -> Value {
        self.state(actor_id).executable_balance
    }

    pub(crate) fn top_up_executable_balance(&mut self, actor_id: ActorId, value: Value) {
        self.modify_state(actor_id, |state| {
            state.executable_balance = state
                .executable_balance
                .checked_add(value)
                .expect("executable balance overflow");
        });

        log::debug!(
            target: "gtest::ethexe",
            "Topped up ethexe executable balance for {actor_id}: +{value}, total={}",
            self.executable_balance_of(actor_id)
        );
    }

    pub(crate) fn top_up_owned_balance(&mut self, actor_id: ActorId, value: Value) {
        self.modify_state(actor_id, |state| {
            state.balance = state
                .balance
                .checked_add(value)
                .expect("owned balance overflow");
        });

        log::debug!(
            target: "gtest::ethexe",
            "Topped up ethexe owned balance for {actor_id}: +{value}, total={}",
            self.balance_of(actor_id)
        );
    }

    fn modify_state(&mut self, actor_id: ActorId, f: impl FnOnce(&mut ProgramState)) {
        let entry = self
            .program_states
            .get_mut(&actor_id)
            .unwrap_or_else(|| panic!("ethexe program {actor_id} not found"));
        let mut state = self
            .db
            .program_state(entry.hash)
            .expect("ethexe program state missing from database");

        f(&mut state);

        entry.canonical_queue_size = state.canonical_queue.cached_queue_size;
        entry.injected_queue_size = state.injected_queue.cached_queue_size;
        entry.hash = self.db.write_program_state(state);
    }

    pub(crate) fn send(
        &mut self,
        source: ActorId,
        destination: ActorId,
        payload: Vec<u8>,
        value: Value,
    ) -> MessageId {
        if !self.is_program(destination) {
            usage_panic!("User message can't be sent to non active ethexe program");
        }
        let payload_len = payload.len();

        let message_id = MessageId::generate_from_user(
            self.block.header.height.saturating_add(1),
            source,
            self.message_nonce as u128,
        );
        self.message_nonce = self.message_nonce.saturating_add(1);

        self.pending_events.push(BlockRequestEvent::Mirror {
            actor_id: destination,
            event: MirrorRequestEvent::MessageQueueingRequested(MessageQueueingRequestedEvent {
                id: message_id,
                source,
                payload,
                value,
                call_reply: false,
            }),
        });

        log::debug!(
            target: "gtest::ethexe",
            "Queued ethexe message {message_id}: source={source}, destination={destination}, payload_len={}, value={value}",
            payload_len
        );

        message_id
    }

    pub(crate) fn push_event(&mut self, event: BlockRequestEvent) {
        log::debug!(target: "gtest::ethexe", "Queued raw ethexe event: {event:?}");
        self.pending_events.push(event);
    }

    pub(crate) fn push_injected_transaction(&mut self, tx: InjectedTransaction) {
        log::debug!(
            target: "gtest::ethexe",
            "Queued raw ethexe injected transaction: destination={}, payload_len={}, value={}",
            tx.destination,
            tx.payload.len(),
            tx.value
        );

        let signer = gsigner::secp256k1::Signer::memory();
        let public_key = signer
            .generate()
            .expect("failed to generate gtest ethexe injected transaction key");
        let tx = signer
            .signed_data(public_key, tx, None)
            .expect("failed to sign gtest ethexe injected transaction")
            .into_verified();

        self.pending_injected.push(tx);
    }

    pub(crate) fn run_new_block(&mut self, allowance: Gas) -> BlockRunResult {
        let balances_before = self.executable_balances();
        let pending_events = self.pending_events.len();
        let pending_injected = self.pending_injected.len();

        self.block.header.height = self.block.header.height.saturating_add(1);
        self.block.header.timestamp = self.block.header.timestamp.saturating_add(12_000);
        self.block.hash = gear_core::utils::hash(&self.block.header.height.to_le_bytes()).into();

        log::debug!(
            target: "gtest::ethexe",
            "Running ethexe block #{}: allowance={allowance}, pending_events={pending_events}, pending_injected={pending_injected}, programs={}",
            self.block.header.height,
            self.program_states.len()
        );

        let executable = ExecutableData {
            block: self.block,
            program_states: self.program_states.clone(),
            schedule: self.schedule.clone(),
            injected_transactions: core::mem::take(&mut self.pending_injected),
            gas_allowance: Some(allowance),
            events: core::mem::take(&mut self.pending_events),
        };

        let finalized = block_on(self.processor.process_programs(executable, None))
            .expect("failed to run ethexe block in gtest");

        self.program_states = finalized.states;
        self.schedule = finalized.schedule;
        self.last_executable_balance_burned =
            executable_balance_burned(balances_before, self.executable_balances());

        let log = transitions_to_logs(&finalized.transitions);
        let succeed = infer_successes(&finalized.transitions);
        let failed = infer_failures(&finalized.transitions);
        let total_processed = u32::try_from(succeed.len() + failed.len())
            .expect("processed ethexe message count exceeds u32");

        log::debug!(
            target: "gtest::ethexe",
            "Finished ethexe block #{}: transitions={}, logs={}, succeed={}, failed={}, executable_balance_burned={}",
            self.block.header.height,
            finalized.transitions.len(),
            log.len(),
            succeed.len(),
            failed.len(),
            self.last_executable_balance_burned
        );
        log::trace!(
            target: "gtest::ethexe",
            "Ethexe block #{} transitions: {:#?}",
            self.block.header.height,
            finalized.transitions
        );

        BlockRunResult {
            block_info: BlockInfo {
                height: self.block.header.height,
                timestamp: self.block.header.timestamp,
            },
            gas_allowance_spent: 0,
            succeed,
            failed,
            not_executed: Default::default(),
            total_processed,
            log,
            gas_burned: Default::default(),
            ethexe_executable_balance_burned: self.last_executable_balance_burned,
        }
    }

    pub(crate) fn calculate_reply_for_handle(
        &self,
        source: ActorId,
        program_id: ActorId,
        payload: Vec<u8>,
        value: Value,
        gas_allowance: Gas,
    ) -> Result<ReplyInfo, String> {
        let state = self
            .program_states
            .get(&program_id)
            .ok_or_else(|| format!("Program state hash for {program_id} not found"))
            .and_then(|state| {
                self.db
                    .program_state(state.hash)
                    .ok_or_else(|| format!("Program state for {program_id} not found"))
            })?;

        if state.requires_init_message() {
            return Err(format!("Program {program_id} is not initialized"));
        }

        log::debug!(
            target: "gtest::ethexe",
            "Calculating ethexe reply: source={source}, program_id={program_id}, payload_len={}, value={value}, gas_allowance={gas_allowance}",
            payload.len()
        );

        // gtest reply calculation must not commit storage writes or queued dispatches.
        let db = unsafe { self.db.clone().overlaid() };
        let mut processor = Processor::new(db.clone()).map_err(|err| err.to_string())?;
        let program_states = self.program_states_without_queues(&db);
        let mut block = self.block;
        block.header.height = block.header.height.saturating_add(1);
        block.header.timestamp = block.header.timestamp.saturating_add(12_000);
        block.hash = gear_core::utils::hash(&block.header.height.to_le_bytes()).into();
        let message_id = MessageId::generate_from_user(block.header.height, source, u128::MAX);

        let finalized = block_on(processor.process_programs(
            ExecutableData {
                block,
                program_states,
                schedule: Default::default(),
                injected_transactions: Default::default(),
                gas_allowance: Some(gas_allowance),
                events: vec![BlockRequestEvent::Mirror {
                    actor_id: program_id,
                    event: MirrorRequestEvent::MessageQueueingRequested(
                        MessageQueueingRequestedEvent {
                            id: message_id,
                            source,
                            payload,
                            value,
                            call_reply: false,
                        },
                    ),
                }],
            },
            None,
        ))
        .map_err(|err| err.to_string())?;

        let reply = finalized
            .transitions
            .iter()
            .flat_map(|transition| transition.messages.iter())
            .find_map(|message| {
                message.reply_details.and_then(|details| {
                    (details.to_message_id() == message_id).then(|| ReplyInfo {
                        payload: message.payload.clone(),
                        value: message.value,
                        code: details.to_reply_code(),
                    })
                })
            })
            .ok_or_else(|| "Reply not found".to_string())?;

        log::debug!(
            target: "gtest::ethexe",
            "Calculated ethexe reply for {message_id}: code={:?}, payload_len={}, value={}",
            reply.code,
            reply.payload.len(),
            reply.value
        );

        Ok(reply)
    }

    fn executable_balances(&self) -> BTreeMap<ActorId, Value> {
        self.program_states
            .keys()
            .copied()
            .map(|actor_id| (actor_id, self.executable_balance_of(actor_id)))
            .collect()
    }

    fn program_states_without_queues(&self, db: &Database) -> ProgramStates {
        let mut program_states = self.program_states.clone();

        for state_hash in program_states.values_mut() {
            let mut state = db
                .program_state(state_hash.hash)
                .expect("ethexe program state missing from database");
            let empty = ProgramState::zero();
            state.canonical_queue = empty.canonical_queue;
            state.injected_queue = empty.injected_queue;

            state_hash.hash = db.write_program_state(state);
            state_hash.canonical_queue_size = 0;
            state_hash.injected_queue_size = 0;
        }

        program_states
    }
}

fn transitions_to_logs(transitions: &[StateTransition]) -> Vec<crate::log::CoreLog> {
    transitions
        .iter()
        .flat_map(|transition| {
            transition.messages.iter().map(|message| {
                let reply_code = message.reply_details.map(|details| details.to_reply_code());
                let reply_to = message.reply_details.map(|details| details.to_message_id());
                crate::log::CoreLog::new(
                    message.id,
                    transition.actor_id,
                    message.destination,
                    message.payload.clone(),
                    reply_code,
                    reply_to,
                )
            })
        })
        .collect()
}

fn infer_successes(transitions: &[StateTransition]) -> BTreeSet<MessageId> {
    transitions
        .iter()
        .flat_map(|transition| transition.messages.iter())
        .filter_map(|message| {
            message
                .reply_details
                .filter(|details| matches!(details.to_reply_code(), ReplyCode::Success(_)))
                .map(|details| details.to_message_id())
        })
        .collect()
}

fn infer_failures(transitions: &[StateTransition]) -> BTreeSet<MessageId> {
    transitions
        .iter()
        .flat_map(|transition| transition.messages.iter())
        .filter_map(|message| {
            message
                .reply_details
                .filter(|details| matches!(details.to_reply_code(), ReplyCode::Error(_)))
                .map(|details| details.to_message_id())
        })
        .collect()
}

fn executable_balance_burned(
    before: BTreeMap<ActorId, Value>,
    after: BTreeMap<ActorId, Value>,
) -> Value {
    before
        .into_iter()
        .map(|(actor_id, before)| {
            before.saturating_sub(after.get(&actor_id).copied().unwrap_or_default())
        })
        .sum()
}
