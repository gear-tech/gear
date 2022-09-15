// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
    common::{
        Actor, DispatchOutcome, DispatchResult, DispatchResultKind, ExecutableActorData,
        ExecutionErrorReason, JournalNote, WasmExecutionContext,
    },
    configs::{BlockConfig, ExecutionSettings, MessageExecutionContext},
    executor,
    ext::ProcessorExt,
};
use alloc::{boxed::Box, collections::BTreeMap, string::ToString, vec::Vec};
use codec::Encode;
use gear_backend_common::{Environment, IntoExtInfo};
use gear_core::{
    code::InstrumentedCode,
    env::Ext as EnvExt,
    gas::{GasAllowanceCounter, GasCounter},
    ids::ProgramId,
    memory::{PageBuf, PageNumber, WasmPageNumber},
    message::{DispatchKind, ExitCode, IncomingDispatch, ReplyMessage, StoredDispatch},
    program::Program,
};

enum SuccessfulDispatchResultKind {
    Exit(ProgramId),
    Wait(Option<u32>),
    Success,
}

/// Checked parameters for message execution across processing runs.
pub struct PreparedMessageExecutionContext {
    gas_counter: GasCounter,
    gas_allowance_counter: GasAllowanceCounter,
    dispatch: IncomingDispatch,
    origin: ProgramId,
    balance: u128,
    actor_data: ExecutableActorData,
    memory_size: WasmPageNumber,
}

impl PreparedMessageExecutionContext {
    /// Returns reference to the GasCounter.
    pub fn gas_counter(&self) -> &GasCounter {
        &self.gas_counter
    }

    /// Returns reference to the ExecutableActorData.
    pub fn actor_data(&self) -> &ExecutableActorData {
        &self.actor_data
    }
}

/// Checked parameters for message execution across processing runs.
pub struct ProcessExecutionContext {
    gas_counter: GasCounter,
    gas_allowance_counter: GasAllowanceCounter,
    dispatch: IncomingDispatch,
    origin: ProgramId,
    balance: u128,
    program: Program,
    memory_size: WasmPageNumber,
}

impl
    From<(
        Box<PreparedMessageExecutionContext>,
        ProgramId,
        InstrumentedCode,
    )> for ProcessExecutionContext
{
    fn from(
        args: (
            Box<PreparedMessageExecutionContext>,
            ProgramId,
            InstrumentedCode,
        ),
    ) -> Self {
        let (context, program_id, code) = args;

        let PreparedMessageExecutionContext {
            gas_counter,
            gas_allowance_counter,
            dispatch,
            origin,
            balance,
            actor_data,
            memory_size,
        } = *context;

        let program = Program::from_parts(
            program_id,
            code,
            actor_data.allocations,
            actor_data.initialized,
        );

        Self {
            gas_counter,
            gas_allowance_counter,
            dispatch,
            origin,
            balance,
            program,
            memory_size,
        }
    }
}

/// Defines result variants of the function `prepare`.
pub enum PrepareResult {
    /// Successfully pre-charged for memory pages.
    Ok(Box<PreparedMessageExecutionContext>),
    /// Required function is not exported. The program will not be executed.
    WontExecute {
        /// Array of JournalNote.
        journal: Vec<JournalNote>,
        /// The amount of burned gas.
        gas_burned: u64,
    },
    /// Provided actor is not executable or there is not enough gas for memory pages size.
    Error {
        /// Array of JournalNote.
        journal: Vec<JournalNote>,
        /// The amount of burned gas.
        gas_burned: u64,
    },
}

fn prepare_error(
    dispatch: IncomingDispatch,
    program_id: ProgramId,
    gas_counter: GasCounter,
    err: ExecutionErrorReason,
) -> PrepareResult {
    let gas_burned = gas_counter.burned();
    PrepareResult::Error {
        journal: process_error(dispatch, program_id, gas_burned, err),
        gas_burned,
    }
}

fn prepare_allowance_exceed(
    dispatch: IncomingDispatch,
    program_id: ProgramId,
    gas_counter: GasCounter,
) -> PrepareResult {
    let gas_burned = gas_counter.burned();
    PrepareResult::Error {
        journal: process_allowance_exceed(dispatch, program_id, gas_burned),
        gas_burned,
    }
}

/// Prepares environment for the execution of a program.
/// Checks if there is a required export and tries to pre-charge for memory pages.
/// Returns either `PreparedMessageExecutionContext` for `process` or an array of journal notes.
/// See `PrepareResult` for details.
pub fn prepare(
    block_config: &BlockConfig,
    execution_context: MessageExecutionContext,
) -> PrepareResult {
    use executor::ChargeForBytesResult;

    let per_byte_cost = block_config.per_byte_cost;
    let read_cost = block_config.read_cost;

    let MessageExecutionContext {
        actor,
        dispatch,
        origin,
        gas_allowance,
        subsequent_execution,
        subsequent_code_loading,
    } = execution_context;
    let Actor {
        balance,
        destination_program: program_id,
        executable_data,
    } = actor;

    let actor_data = match check_is_executable(executable_data, &dispatch) {
        Err(exit_code) => {
            return PrepareResult::Error {
                journal: process_non_executable(dispatch, program_id, exit_code),
                gas_burned: 0,
            };
        }
        Ok(data) => data,
    };

    let mut gas_counter = GasCounter::new(dispatch.gas_limit());
    let mut gas_allowance_counter = GasAllowanceCounter::new(gas_allowance);

    match executor::charge_gas_for_program(
        read_cost,
        per_byte_cost,
        &mut gas_counter,
        &mut gas_allowance_counter,
        subsequent_execution,
    ) {
        ChargeForBytesResult::Ok => (),
        ChargeForBytesResult::GasExceeded => {
            // program struct has already been loaded so charge anyway
            gas_counter.charge(gas_counter.left());

            return prepare_error(
                dispatch,
                program_id,
                gas_counter,
                ExecutionErrorReason::ProgramDataGasExceeded,
            );
        }
        ChargeForBytesResult::BlockGasExceeded => {
            return prepare_allowance_exceed(dispatch, program_id, gas_counter)
        }
    }

    if !actor_data.code_exports.contains(&dispatch.kind()) {
        let gas_burned = gas_counter.burned();
        return PrepareResult::WontExecute {
            journal: process_success(
                SuccessfulDispatchResultKind::Success,
                DispatchResult::success(dispatch, program_id, gas_counter.into()),
            ),
            gas_burned,
        };
    }

    match executor::charge_gas_for_code(
        read_cost,
        per_byte_cost,
        actor_data.code_length_bytes,
        &mut gas_counter,
        &mut gas_allowance_counter,
        subsequent_code_loading,
    ) {
        ChargeForBytesResult::Ok => (),
        ChargeForBytesResult::GasExceeded => {
            return prepare_error(
                dispatch,
                program_id,
                gas_counter,
                ExecutionErrorReason::ProgramCodeGasExceeded,
            );
        }
        ChargeForBytesResult::BlockGasExceeded => {
            return prepare_allowance_exceed(dispatch, program_id, gas_counter);
        }
    }

    let memory_size = match executor::charge_gas_for_pages(
        &block_config.allocations_config,
        &mut gas_counter,
        &mut gas_allowance_counter,
        &actor_data.allocations,
        actor_data.static_pages,
        dispatch.context().is_none() && matches!(dispatch.kind(), DispatchKind::Init),
        subsequent_execution,
    ) {
        Ok(size) => {
            log::debug!("Charged for memory pages. Size: {size:?}");
            size
        }
        Err(reason) => {
            log::debug!("Failed to charge for memory pages: {reason:?}");
            return match reason {
                ExecutionErrorReason::InitialMemoryBlockGasExceeded
                | ExecutionErrorReason::GrowMemoryBlockGasExceeded
                | ExecutionErrorReason::LoadMemoryBlockGasExceeded => {
                    prepare_allowance_exceed(dispatch, program_id, gas_counter)
                }
                _ => prepare_error(dispatch, program_id, gas_counter, reason),
            };
        }
    };

    PrepareResult::Ok(Box::new(PreparedMessageExecutionContext {
        gas_counter,
        gas_allowance_counter,
        dispatch,
        origin,
        balance,
        actor_data,
        memory_size,
    }))
}

/// Process program & dispatch for it and return journal for updates.
pub fn process<A: ProcessorExt + EnvExt + IntoExtInfo + 'static, E: Environment<A>>(
    block_config: &BlockConfig,
    execution_context: ProcessExecutionContext,
    memory_pages: BTreeMap<PageNumber, PageBuf>,
) -> Vec<JournalNote> {
    use SuccessfulDispatchResultKind::*;

    let BlockConfig {
        block_info,
        allocations_config,
        existential_deposit,
        outgoing_limit,
        host_fn_weights,
        forbidden_funcs,
        mailbox_threshold,
        waitlist_cost,
        reserve_for,
        ..
    } = block_config.clone();

    let execution_settings = ExecutionSettings {
        block_info,
        existential_deposit,
        allocations_config,
        host_fn_weights,
        forbidden_funcs,
        mailbox_threshold,
        waitlist_cost,
        reserve_for,
    };

    let dispatch = execution_context.dispatch;
    let balance = execution_context.balance;
    let program_id = execution_context.program.id();
    let execution_context = WasmExecutionContext {
        origin: execution_context.origin,
        gas_counter: execution_context.gas_counter,
        gas_allowance_counter: execution_context.gas_allowance_counter,
        program: execution_context.program,
        pages_initial_data: memory_pages,
        memory_size: execution_context.memory_size,
    };
    let msg_ctx_settings = gear_core::message::ContextSettings::new(0, outgoing_limit);

    let exec_result = executor::execute_wasm::<A, E>(
        balance,
        dispatch.clone(),
        execution_context,
        execution_settings,
        msg_ctx_settings,
    )
    .map_err(|err| {
        log::debug!("Wasm execution error: {}", err.reason);
        err
    });

    match exec_result {
        Ok(res) => match res.kind {
            DispatchResultKind::Trap(reason) => process_error(
                res.dispatch,
                program_id,
                res.gas_amount.burned(),
                ExecutionErrorReason::Ext(reason),
            ),
            DispatchResultKind::Success => process_success(Success, res),
            DispatchResultKind::Wait(duration) => process_success(Wait(duration), res),
            DispatchResultKind::Exit(value_destination) => {
                process_success(Exit(value_destination), res)
            }
            DispatchResultKind::GasAllowanceExceed => {
                process_allowance_exceed(dispatch, program_id, res.gas_amount.burned())
            }
        },
        Err(e) => match e.reason {
            ExecutionErrorReason::InitialMemoryBlockGasExceeded
            | ExecutionErrorReason::GrowMemoryBlockGasExceeded
            | ExecutionErrorReason::LoadMemoryBlockGasExceeded => {
                process_allowance_exceed(dispatch, program_id, e.gas_amount.burned())
            }
            _ => process_error(dispatch, program_id, e.gas_amount.burned(), e.reason),
        },
    }
}

fn check_is_executable(
    executable_data: Option<ExecutableActorData>,
    dispatch: &IncomingDispatch,
) -> Result<ExecutableActorData, ExitCode> {
    executable_data
        .map(|data| {
            if data.initialized & matches!(dispatch.kind(), DispatchKind::Init) {
                Err(crate::RE_INIT_EXIT_CODE)
            } else {
                Ok(data)
            }
        })
        .unwrap_or(Err(crate::UNAVAILABLE_DEST_EXIT_CODE))
}

/// Helper function for journal creation in trap/error case
fn process_error(
    dispatch: IncomingDispatch,
    program_id: ProgramId,
    gas_burned: u64,
    err: ExecutionErrorReason,
) -> Vec<JournalNote> {
    let mut journal = Vec::new();

    let message_id = dispatch.id();
    let origin = dispatch.source();
    let value = dispatch.value();

    journal.push(JournalNote::GasBurned {
        message_id,
        amount: gas_burned,
    });

    // We check if value is greater than zero to don't provide
    // no-op journal note.
    //
    // We also check if dispatch had context of previous executions:
    // it's existence shows that we have processed message after
    // being waken, so the value were already transferred in
    // execution, where `gr_wait` was called.
    if dispatch.context().is_none() && value != 0 {
        // Send back value
        journal.push(JournalNote::SendValue {
            from: origin,
            to: None,
            value,
        });
    }

    if !dispatch.is_error_reply() {
        // # Safety
        //
        // 1. The dispatch.id() has already been checked
        // 2. This reply message is generated by our system
        //
        // So, the message id of this reply message will not be duplicated.
        let message = ReplyMessage::system(dispatch.id(), err.encode(), crate::ERR_EXIT_CODE);

        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch: message.into_dispatch(program_id, dispatch.source(), dispatch.id()),
        });
    }

    let outcome = match dispatch.kind() {
        DispatchKind::Init => DispatchOutcome::InitFailure {
            program_id,
            origin,
            reason: err.to_string(),
        },
        _ => DispatchOutcome::MessageTrap {
            program_id,
            trap: err.to_string(),
        },
    };

    journal.push(JournalNote::MessageDispatched {
        message_id,
        source: origin,
        outcome,
    });
    journal.push(JournalNote::MessageConsumed(message_id));

    journal
}

/// Helper function for journal creation in success case
fn process_success(
    kind: SuccessfulDispatchResultKind,
    dispatch_result: DispatchResult,
) -> Vec<JournalNote> {
    use SuccessfulDispatchResultKind::*;

    let DispatchResult {
        dispatch,
        generated_dispatches,
        awakening,
        program_candidates,
        gas_amount,
        page_update,
        program_id,
        context_store,
        allocations,
        ..
    } = dispatch_result;

    let mut journal = Vec::new();

    let message_id = dispatch.id();
    let origin = dispatch.source();
    let value = dispatch.value();

    journal.push(JournalNote::GasBurned {
        message_id,
        amount: gas_amount.burned(),
    });

    // We check if value is greater than zero to don't provide
    // no-op journal note.
    //
    // We also check if dispatch had context of previous executions:
    // it's existence shows that we have processed message after
    // being waken, so the value were already transferred in
    // execution, where `gr_wait` was called.
    if dispatch.context().is_none() && value != 0 {
        // Send value further
        journal.push(JournalNote::SendValue {
            from: origin,
            to: Some(program_id),
            value,
        });
    }

    // Must be handled before handling generated dispatches.
    for (code_hash, candidates) in program_candidates {
        journal.push(JournalNote::StoreNewPrograms {
            code_hash,
            candidates,
        });
    }

    for dispatch in generated_dispatches {
        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch,
        });
    }

    for awakening_id in awakening {
        journal.push(JournalNote::WakeMessage {
            message_id,
            program_id,
            awakening_id,
        });
    }

    for (page_number, data) in page_update {
        journal.push(JournalNote::UpdatePage {
            program_id,
            page_number,
            data,
        })
    }

    if let Some(allocations) = allocations {
        journal.push(JournalNote::UpdateAllocations {
            program_id,
            allocations,
        });
    }

    let outcome = match kind {
        Wait(duration) => {
            journal.push(JournalNote::WaitDispatch {
                dispatch: dispatch.into_stored(program_id, context_store),
                duration,
            });

            return journal;
        }
        Success => match dispatch.kind() {
            DispatchKind::Init => DispatchOutcome::InitSuccess { program_id },
            _ => DispatchOutcome::Success,
        },
        Exit(value_destination) => {
            journal.push(JournalNote::ExitDispatch {
                id_exited: program_id,
                value_destination,
            });

            DispatchOutcome::Exit { program_id }
        }
    };

    journal.push(JournalNote::MessageDispatched {
        message_id,
        source: origin,
        outcome,
    });
    journal.push(JournalNote::MessageConsumed(message_id));
    journal
}

fn process_allowance_exceed(
    dispatch: IncomingDispatch,
    program_id: ProgramId,
    gas_burned: u64,
) -> Vec<JournalNote> {
    let mut journal = Vec::with_capacity(1);

    let (kind, message, opt_context) = dispatch.into_parts();

    let dispatch = StoredDispatch::new(kind, message.into_stored(program_id), opt_context);

    journal.push(JournalNote::StopProcessing {
        dispatch,
        gas_burned,
    });

    journal
}

/// Helper function for journal creation in message no execution case
fn process_non_executable(
    dispatch: IncomingDispatch,
    program_id: ProgramId,
    exit_code: ExitCode,
) -> Vec<JournalNote> {
    // Number of notes is predetermined
    let mut journal = Vec::with_capacity(4);

    let message_id = dispatch.id();
    let source = dispatch.source();
    let value = dispatch.value();

    if value != 0 {
        // Send value back
        journal.push(JournalNote::SendValue {
            from: dispatch.source(),
            to: None,
            value,
        });
    }

    // Reply back to the message `source`
    if !dispatch.is_error_reply() {
        // # Safety
        //
        // 1. The dispatch.id() has already been checked
        // 2. This reply message is generated by our system
        //
        // So, the message id of this reply message will not be duplicated.
        let message = ReplyMessage::system(
            dispatch.id(),
            ExecutionErrorReason::NonExecutable.encode(),
            exit_code,
        );

        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch: message.into_dispatch(program_id, dispatch.source(), dispatch.id()),
        });
    }

    journal.push(JournalNote::MessageDispatched {
        message_id,
        source,
        outcome: DispatchOutcome::NoExecution,
    });

    journal.push(JournalNote::MessageConsumed(message_id));

    journal
}
