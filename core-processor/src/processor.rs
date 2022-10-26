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
        ExecutionErrorReason, JournalNote, PrechargedDispatch, WasmExecutionContext,
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
    message::{
        ContextSettings, DispatchKind, ExitCode, IncomingDispatch, MessageWaitedType, ReplyMessage,
        StoredDispatch,
    },
    program::Program,
    reservation::{GasReservationState, GasReserver},
};

#[derive(Debug)]
enum SuccessfulDispatchResultKind {
    Exit(ProgramId),
    Wait(Option<u32>, MessageWaitedType),
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
    max_reservations: u64,
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
    gas_reserver: GasReserver,
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
            mut dispatch,
            origin,
            balance,
            actor_data,
            memory_size,
            max_reservations,
        } = *context;

        let program = Program::from_parts(
            program_id,
            code,
            actor_data.allocations,
            actor_data.initialized,
        );

        let gas_reserver = GasReserver::new(
            dispatch.id(),
            dispatch
                .context_mut()
                .as_mut()
                .map(|ctx| ctx.fetch_inc_reservation_nonce())
                .unwrap_or(0),
            actor_data.gas_reservation_map,
            max_reservations,
        );

        Self {
            gas_counter,
            gas_allowance_counter,
            gas_reserver,
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
    /// Successfully pre-charged for resources.
    Ok(Box<PreparedMessageExecutionContext>),
    /// Required function is not exported. The program will not be executed.
    WontExecute(Vec<JournalNote>),
    /// Provided actor is not executable or there is not enough gas for resources.
    Error(Vec<JournalNote>),
}

fn prepare_error(
    dispatch: IncomingDispatch,
    program_id: ProgramId,
    gas_counter: GasCounter,
    err: ExecutionErrorReason,
) -> PrepareResult {
    let gas_burned = gas_counter.burned();
    PrepareResult::Error(process_error(dispatch, program_id, gas_burned, err))
}

fn prepare_allowance_exceed(
    dispatch: IncomingDispatch,
    program_id: ProgramId,
    gas_counter: GasCounter,
) -> PrepareResult {
    let gas_burned = gas_counter.burned();
    PrepareResult::Error(process_allowance_exceed(dispatch, program_id, gas_burned))
}

/// Defines result variants of the function `precharge`.
pub enum PrechargeResult {
    /// Successfully pre-charged for resources.
    Ok(PrechargedDispatch),
    /// There is not enough gas for resources.
    Error(Vec<JournalNote>),
}

/// Charge a message for program data beforehand.
pub fn precharge(
    block_config: &BlockConfig,
    gas_allowance: u64,
    dispatch: IncomingDispatch,
    destination_id: ProgramId,
) -> PrechargeResult {
    use executor::ChargeForBytesResult::*;

    let read_per_byte_cost = block_config.read_per_byte_cost;
    let read_cost = block_config.read_cost;

    let mut gas_counter = GasCounter::new(dispatch.gas_limit());
    let mut gas_allowance_counter = GasAllowanceCounter::new(gas_allowance);

    match executor::charge_gas_for_program(
        read_cost,
        read_per_byte_cost,
        &mut gas_counter,
        &mut gas_allowance_counter,
    ) {
        Ok => PrechargeResult::Ok((dispatch, gas_counter, gas_allowance_counter).into()),
        GasExceeded => {
            let gas_burned = gas_counter.burned();
            PrechargeResult::Error(process_error(
                dispatch,
                destination_id,
                gas_burned,
                ExecutionErrorReason::ProgramDataGasExceeded,
            ))
        }
        BlockGasExceeded => {
            let gas_burned = gas_counter.burned();
            PrechargeResult::Error(process_allowance_exceed(
                dispatch,
                destination_id,
                gas_burned,
            ))
        }
    }
}

/// Prepares environment for the execution of a program.
/// Checks if there is a required export and tries to charge for code and memory pages.
/// Returns either `PreparedMessageExecutionContext` for `process` or an array of journal notes.
/// See `PrepareResult` for details.
pub fn prepare(
    block_config: &BlockConfig,
    execution_context: MessageExecutionContext,
) -> PrepareResult {
    use executor::ChargeForBytesResult;

    let read_per_byte_cost = block_config.read_per_byte_cost;
    let read_cost = block_config.read_cost;
    let max_reservations = block_config.max_reservations;

    let MessageExecutionContext {
        actor,
        precharged_dispatch,
        origin,
        subsequent_execution,
    } = execution_context;
    let Actor {
        balance,
        destination_program: program_id,
        executable_data,
    } = actor;

    let (dispatch, mut gas_counter, mut gas_allowance_counter) = precharged_dispatch.into_parts();

    let actor_data = match check_is_executable(executable_data, &dispatch) {
        Err(exit_code) => {
            return PrepareResult::Error(process_non_executable(dispatch, program_id, exit_code));
        }
        Ok(data) => data,
    };

    if !actor_data.code_exports.contains(&dispatch.kind()) {
        return PrepareResult::WontExecute(process_success(
            SuccessfulDispatchResultKind::Success,
            DispatchResult::success(dispatch, program_id, gas_counter.into()),
        ));
    }

    match executor::charge_gas_for_code(
        read_cost,
        read_per_byte_cost,
        actor_data.code_length_bytes,
        &mut gas_counter,
        &mut gas_allowance_counter,
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

    let mut f = || {
        let memory_size = executor::charge_gas_for_pages(
            &block_config.allocations_config,
            &mut gas_counter,
            &mut gas_allowance_counter,
            &actor_data.allocations,
            actor_data.static_pages,
            dispatch.context().is_none() && matches!(dispatch.kind(), DispatchKind::Init),
            subsequent_execution,
        )?;
        executor::charge_gas_for_instantiation(
            block_config.module_instantiation_byte_cost,
            actor_data.code_length_bytes,
            &mut gas_counter,
            &mut gas_allowance_counter,
        )?;
        Ok(memory_size)
    };
    let memory_size = match f() {
        Ok(size) => {
            log::debug!("Charged for module instantiation and memory pages. Size: {size:?}");
            size
        }
        Err(reason) => {
            log::debug!("Failed to charge for module instantiation or memory pages: {reason:?}");
            return match reason {
                ExecutionErrorReason::InitialMemoryBlockGasExceeded
                | ExecutionErrorReason::GrowMemoryBlockGasExceeded
                | ExecutionErrorReason::LoadMemoryBlockGasExceeded
                | ExecutionErrorReason::ModuleInstantiationBlockGasExceeded => {
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
        max_reservations,
    }))
}

/// Process program & dispatch for it and return journal for updates.
pub fn process<
    A: ProcessorExt + EnvExt + IntoExtInfo<<A as EnvExt>::Error> + 'static,
    E: Environment<A>,
>(
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
        reservation,
        write_cost,
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
        reservation,
    };

    let dispatch = execution_context.dispatch;
    let balance = execution_context.balance;
    let program_id = execution_context.program.id();
    let execution_context = WasmExecutionContext {
        origin: execution_context.origin,
        gas_counter: execution_context.gas_counter,
        gas_allowance_counter: execution_context.gas_allowance_counter,
        gas_reserver: execution_context.gas_reserver,
        program: execution_context.program,
        pages_initial_data: memory_pages,
        memory_size: execution_context.memory_size,
    };

    // Sending fee: double write cost for addition and removal some time soon
    // from queue.
    //
    // Scheduled sending fee: double write cost for addition and removal some time soon
    // from queue and double write cost (addition and removal) for dispatch stash.
    //
    // Waiting fee: triple write cost for addition and removal some time soon
    // from waitlist and enqueuing / sending error reply afterward.
    //
    // Waking fee: double write cost for removal from waitlist
    // and further enqueueing.
    let msg_ctx_settings = ContextSettings::new(
        write_cost.saturating_mul(2),
        write_cost.saturating_mul(4),
        write_cost.saturating_mul(3),
        write_cost.saturating_mul(2),
        outgoing_limit,
    );

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
            DispatchResultKind::Wait(duration, ref waited_type) => {
                process_success(Wait(duration, waited_type.clone()), res)
            }
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
        // This expect panic is unreachable, unless error message is too large or max payload size is too small.
        let err_payload = err.encode().try_into().expect("Error message is too large");
        // # Safety
        //
        // 1. The dispatch.id() has already been checked
        // 2. This reply message is generated by our system
        //
        // So, the message id of this reply message will not be duplicated.
        let dispatch = ReplyMessage::system(dispatch.id(), err_payload, crate::ERR_EXIT_CODE)
            .into_dispatch(program_id, dispatch.source(), dispatch.id());

        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch,
            delay: 0,
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
        gas_reserver,
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

    if let Some(gas_reserver) = gas_reserver {
        journal.extend(gas_reserver.states().iter().flat_map(
            |(&reservation_id, &state)| match state {
                GasReservationState::Exists { .. } => None,
                GasReservationState::Created { amount, duration } => {
                    Some(JournalNote::ReserveGas {
                        message_id,
                        reservation_id,
                        program_id,
                        amount,
                        duration,
                    })
                }
                GasReservationState::Removed { expiration } => Some(JournalNote::UnreserveGas {
                    reservation_id,
                    program_id,
                    expiration,
                }),
            },
        ));

        journal.push(JournalNote::UpdateGasReservations {
            program_id,
            reserver: gas_reserver,
        });
    }

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
    for (code_id, candidates) in program_candidates {
        journal.push(JournalNote::StoreNewPrograms {
            code_id,
            candidates,
        });
    }

    for (dispatch, delay) in generated_dispatches {
        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch,
            delay,
        });
    }

    for (awakening_id, delay) in awakening {
        journal.push(JournalNote::WakeMessage {
            message_id,
            program_id,
            awakening_id,
            delay,
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
        Wait(duration, waited_type) => {
            journal.push(JournalNote::WaitDispatch {
                dispatch: dispatch.into_stored(program_id, context_store),
                duration,
                waited_type,
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
        // This expect panic is unreachable, unless error message is too large or max payload size is too small.
        let err_payload = ExecutionErrorReason::NonExecutable
            .encode()
            .try_into()
            .expect("Error message is too large");
        // # Safety
        //
        // 1. The dispatch.id() has already been checked
        // 2. This reply message is generated by our system
        //
        // So, the message id of this reply message will not be duplicated.
        let dispatch = ReplyMessage::system(dispatch.id(), err_payload, exit_code).into_dispatch(
            program_id,
            dispatch.source(),
            dispatch.id(),
        );

        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch,
            delay: 0,
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
