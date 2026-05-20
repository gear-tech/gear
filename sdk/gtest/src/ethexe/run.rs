// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use super::{backend::EthexeBackend, runtime::GTestEthexeRuntime};
use crate::{
    Gas,
    log::{BlockRunResult, CoreLog},
};
use core_processor::common::DispatchOutcome;
use ethexe_common::{
    PromisePolicy, StateHashWithQueueSize,
    gear::{CHUNK_PROCESSING_GAS_LIMIT, MessageType},
};
use ethexe_runtime_common::{
    BlockInfo, InBlockTransitions, JournalHandler, MAX_CALL_REPLIES_PER_RUN,
    MAX_OUTGOING_MESSAGES_BYTES_PER_RUN, MAX_OUTGOING_MESSAGES_PER_RUN, ProcessQueueContext,
    RuntimeQueueReport, ScheduleHandler, TransitionController, process_queue_with_report,
    state::Storage,
};
use gear_core::{gas::GasAllowanceCounter, ids::ActorId};

// Keep this local to avoid depending on `ethexe-processor`.
const DEFAULT_CHUNK_SIZE: usize = 16;

impl EthexeBackend {
    pub(crate) fn run_new_block(
        &mut self,
        height: u32,
        timestamp: u64,
        allowance: Gas,
    ) -> BlockRunResult {
        let block_info = BlockInfo { height, timestamp };
        let mut transitions =
            InBlockTransitions::new(height, self.states.clone(), self.schedule.clone());
        let mut gas_allowance = GasAllowanceCounter::new(allowance);
        let mut result = BlockRunResult {
            block_info,
            ..Default::default()
        };

        self.process_scheduled_tasks(&mut transitions);
        let injected_processed = self.process_queue_type(
            &mut transitions,
            MessageType::Injected,
            block_info,
            &mut gas_allowance,
            &mut result,
        );

        if injected_processed && gas_allowance.left() != 0 {
            self.process_queue_type(
                &mut transitions,
                MessageType::Canonical,
                block_info,
                &mut gas_allowance,
                &mut result,
            );
        }

        result.log.extend(
            transitions
                .current_messages()
                .into_iter()
                .map(|(source, message)| CoreLog::from_ethexe_message(source, message)),
        );

        let finalized = transitions.finalize();
        self.states = finalized.states;
        self.schedule = finalized.schedule;
        result.gas_allowance_spent = allowance.saturating_sub(gas_allowance.left());

        result
    }

    pub(crate) fn run_scheduled_block(&mut self, height: u32, timestamp: u64) -> BlockRunResult {
        let block_info = BlockInfo { height, timestamp };
        let mut transitions =
            InBlockTransitions::new(height, self.states.clone(), self.schedule.clone());
        let mut result = BlockRunResult {
            block_info,
            ..Default::default()
        };

        self.process_scheduled_tasks(&mut transitions);
        result.log.extend(
            transitions
                .current_messages()
                .into_iter()
                .map(|(source, message)| CoreLog::from_ethexe_message(source, message)),
        );

        let finalized = transitions.finalize();
        self.states = finalized.states;
        self.schedule = finalized.schedule;

        result
    }

    fn process_scheduled_tasks(&self, transitions: &mut InBlockTransitions) {
        let tasks = transitions.take_actual_tasks();
        let mut handler = ScheduleHandler {
            controller: TransitionController {
                storage: &self.storage,
                transitions,
            },
        };

        for task in tasks {
            let _gas = task.process_with(&mut handler);
        }
    }

    fn process_queue_type(
        &self,
        transitions: &mut InBlockTransitions,
        queue_type: MessageType,
        block_info: BlockInfo,
        gas_allowance: &mut GasAllowanceCounter,
        result: &mut BlockRunResult,
    ) -> bool {
        loop {
            if gas_allowance.left() == 0 {
                break;
            }

            let chunks = execution_chunks(transitions, queue_type);
            if chunks.is_empty() {
                break;
            }

            for chunk in chunks {
                if gas_allowance.left() == 0 {
                    break;
                }

                let chunk_allowance = gas_allowance.left().min(CHUNK_PROCESSING_GAS_LIMIT);
                let mut chunk_journals = Vec::with_capacity(chunk.len());
                let mut max_gas_spent_in_chunk = 0;
                let mut reports_empty = true;

                for (program_id, state) in chunk {
                    let (instrumented_code, code_metadata) = self.program_code(program_id);
                    let runtime = GTestEthexeRuntime::new(&self.storage, state.hash);
                    let (journals, gas_spent, report) = process_queue_with_report(
                        ProcessQueueContext {
                            program_id,
                            state_root: state.hash,
                            queue_type,
                            instrumented_code,
                            code_metadata,
                            gas_allowance: GasAllowanceCounter::new(chunk_allowance),
                            block_info,
                            // gtest currently models promise syscalls as unavailable in ethexe mode.
                            promise_policy: PromisePolicy::Disabled,
                        },
                        &runtime,
                    );

                    let new_state_hash = runtime.state_hash();
                    let new_state = self
                        .storage
                        .program_state(new_state_hash)
                        .expect("ethexe runtime produced missing program state");
                    transitions.modify_state(
                        program_id,
                        new_state_hash,
                        new_state.canonical_queue.cached_queue_size,
                        new_state.injected_queue.cached_queue_size,
                    );

                    reports_empty &= report.dispatched.is_empty() && report.gas_burned.is_empty();
                    apply_queue_report(result, report);
                    max_gas_spent_in_chunk = max_gas_spent_in_chunk.max(gas_spent);
                    chunk_journals.push((program_id, journals));
                }

                let mut out_of_gas = false;
                let mut outgoing_messages_limiter = MAX_OUTGOING_MESSAGES_PER_RUN;
                let mut outgoing_messages_bytes_limiter = MAX_OUTGOING_MESSAGES_BYTES_PER_RUN;
                let mut call_reply_limiter = MAX_CALL_REPLIES_PER_RUN;

                for (program_id, program_journals) in chunk_journals {
                    for (journal, message_type, call_reply) in program_journals {
                        let mut journal_handler = JournalHandler {
                            program_id,
                            message_type,
                            call_reply,
                            controller: TransitionController {
                                storage: &self.storage,
                                transitions,
                            },
                            gas_allowance_counter: gas_allowance,
                            chunk_gas_limit: CHUNK_PROCESSING_GAS_LIMIT,
                            out_of_gas: &mut out_of_gas,
                            outgoing_messages_limiter: &mut outgoing_messages_limiter,
                            outgoing_messages_bytes_limiter: &mut outgoing_messages_bytes_limiter,
                            call_reply_limiter: &mut call_reply_limiter,
                        };

                        core_processor::handle_journal(journal, &mut journal_handler);
                    }
                }

                let charge_result = gas_allowance.charge(max_gas_spent_in_chunk);
                assert!(
                    charge_result.is_enough(),
                    "Gas allowance counter must be enough after charging chunk gas"
                );

                if out_of_gas {
                    return false;
                }

                if max_gas_spent_in_chunk == 0 && reports_empty {
                    return false;
                }
            }
        }

        true
    }

    fn program_code(
        &self,
        program_id: ActorId,
    ) -> (
        gear_core::code::InstrumentedCode,
        gear_core::code::CodeMetadata,
    ) {
        let code_id = self
            .code_ids
            .get(&program_id)
            .unwrap_or_else(|| panic!("missing ethexe code id for program {program_id:?}"));
        let instrumented_code = self
            .instrumented_codes
            .get(code_id)
            .unwrap_or_else(|| panic!("missing ethexe instrumented code {code_id:?}"))
            .clone();
        let code_metadata = self
            .code_metadata
            .get(code_id)
            .unwrap_or_else(|| panic!("missing ethexe code metadata {code_id:?}"))
            .clone();

        (instrumented_code, code_metadata)
    }
}

fn execution_chunks(
    transitions: &InBlockTransitions,
    queue_type: MessageType,
) -> Vec<Vec<(ActorId, StateHashWithQueueSize)>> {
    let states: Vec<_> = transitions
        .states_iter()
        .filter_map(|(&program_id, &state)| {
            (queue_size(state, queue_type) != 0).then_some((program_id, state))
        })
        .collect();

    if states.is_empty() {
        return Vec::new();
    }

    let chunks_len = states.len().div_ceil(DEFAULT_CHUNK_SIZE);
    let mut chunks = vec![Vec::new(); chunks_len];

    for (program_id, state) in states {
        // Programs with larger queues are placed in earlier chunks to be processed first.
        let chunk_idx = usize::from(queue_size(state, queue_type))
            .min(chunks_len)
            .saturating_sub(1);
        chunks[chunk_idx].push((program_id, state));
    }

    let mut ordered: Vec<_> = chunks.into_iter().flatten().collect();
    ordered.reverse();

    ordered
        .chunks(DEFAULT_CHUNK_SIZE)
        .map(<[_]>::to_vec)
        .collect()
}

fn queue_size(state: StateHashWithQueueSize, queue_type: MessageType) -> u8 {
    match queue_type {
        MessageType::Canonical => state.canonical_queue_size,
        MessageType::Injected => state.injected_queue_size,
    }
}

fn apply_queue_report(result: &mut BlockRunResult, report: RuntimeQueueReport) {
    for dispatch in report.dispatched {
        match dispatch.outcome {
            DispatchOutcome::MessageTrap { .. } | DispatchOutcome::InitFailure { .. } => {
                result.failed.insert(dispatch.message_id);
            }
            DispatchOutcome::NoExecution => {
                result.not_executed.insert(dispatch.message_id);
            }
            DispatchOutcome::Success
            | DispatchOutcome::Exit { .. }
            | DispatchOutcome::InitSuccess { .. } => {
                result.succeed.insert(dispatch.message_id);
            }
        }

        result.total_processed = result.total_processed.saturating_add(1);
    }

    for gas_burned in report.gas_burned {
        if !gas_burned.charged_to_executable_balance {
            continue;
        }

        result
            .gas_burned
            .entry(gas_burned.message_id)
            .and_modify(|amount| *amount = amount.saturating_add(gas_burned.amount))
            .or_insert(gas_burned.amount);
    }
}
