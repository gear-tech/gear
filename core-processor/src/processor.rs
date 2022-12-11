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
        DispatchOutcome, DispatchResult, DispatchResultKind, ExecutableActorData,
        ExecutionErrorReason, GasOperation, JournalNote, PrechargedDispatch, WasmExecutionContext,
    },
    configs::{BlockConfig, ExecutionSettings},
    context::*,
    executor,
    ext::ProcessorExt,
};
use alloc::{collections::BTreeMap, string::ToString, vec::Vec};
use codec::Encode;
use gear_backend_common::{Environment, IntoExtInfo, SystemReservationContext};
use gear_core::{
    env::Ext as EnvExt,
    gas::{GasAllowanceCounter, GasCounter},
    ids::ProgramId,
    memory::{PageBuf, PageNumber},
    message::{
        ContextSettings, DispatchKind, IncomingDispatch, MessageWaitedType, ReplyMessage,
        StatusCode, StoredDispatch,
    },
    reservation::GasReservationState,
};

#[derive(Debug)]
enum SuccessfulDispatchResultKind {
    Exit(ProgramId),
    Wait(Option<u32>, MessageWaitedType),
    Success,
}

/// Defines result variants of the precharge functions.
pub type PrechargeResult<T> = Result<T, Vec<JournalNote>>;

/// Charge a message for program data beforehand.
pub fn precharge_for_program(
    block_config: &BlockConfig,
    gas_allowance: u64,
    dispatch: IncomingDispatch,
    destination_id: ProgramId,
) -> PrechargeResult<PrechargedDispatch> {
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
        Ok => Result::Ok((dispatch, gas_counter, gas_allowance_counter).into()),
        GasExceeded => {
            let gas_burned = gas_counter.burned();
            let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);
            Err(process_error(
                dispatch,
                destination_id,
                gas_burned,
                system_reservation_ctx,
                ExecutionErrorReason::GasExceeded(GasOperation::ProgramData),
                false,
            ))
        }
        BlockGasExceeded => {
            let gas_burned = gas_counter.burned();
            Err(process_allowance_exceed(
                dispatch,
                destination_id,
                gas_burned,
            ))
        }
    }
}

/// Charge a message for fetching the actual length of the binary code
/// from a storage. The updated value of binary code length
/// should be kept in standalone storage. The caller has to call this
/// function to charge gas-counters accrodingly before fetching the value.
///
/// The function also performs several additional checks:
/// - if an actor is executable
/// - if a required dispatch method is exported.
pub fn precharge_for_code_length(
    block_config: &BlockConfig,
    dispatch: PrechargedDispatch,
    destination_id: ProgramId,
    executable_data: Option<ExecutableActorData>,
) -> PrechargeResult<ContextChargedForCodeLength> {
    use executor::ChargeForBytesResult::*;

    let read_cost = block_config.read_cost;

    let (dispatch, mut gas_counter, mut gas_allowance_counter) = dispatch.into_parts();

    let actor_data = match check_is_executable(executable_data, &dispatch) {
        Err(status_code) => {
            return Err(process_non_executable(
                dispatch,
                destination_id,
                status_code,
            ));
        }
        Result::Ok(data) => data,
    };

    if !actor_data.code_exports.contains(&dispatch.kind()) {
        return Err(process_success(
            SuccessfulDispatchResultKind::Success,
            DispatchResult::success(dispatch, destination_id, gas_counter.into()),
        ));
    }

    match executor::charge_gas_for_bytes(read_cost, &mut gas_counter, &mut gas_allowance_counter) {
        Ok => Result::Ok(ContextChargedForCodeLength {
            data: ContextData {
                gas_counter,
                gas_allowance_counter,
                dispatch,
                destination_id,
                actor_data,
            },
        }),
        GasExceeded => {
            let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);
            Err(process_error(
                dispatch,
                destination_id,
                gas_counter.burned(),
                system_reservation_ctx,
                ExecutionErrorReason::GasExceeded(GasOperation::ProgramCode),
                false,
            ))
        }
        BlockGasExceeded => Err(process_allowance_exceed(
            dispatch,
            destination_id,
            gas_counter.burned(),
        )),
    }
}

/// Charge a message for the program binary code beforehand.
pub fn precharge_for_code(
    block_config: &BlockConfig,
    mut context: ContextChargedForCodeLength,
    code_len_bytes: u32,
) -> PrechargeResult<ContextChargedForCode> {
    use executor::ChargeForBytesResult::*;

    let read_per_byte_cost = block_config.read_per_byte_cost;
    let read_cost = block_config.read_cost;

    match executor::charge_gas_for_code(
        read_cost,
        read_per_byte_cost,
        code_len_bytes,
        &mut context.data.gas_counter,
        &mut context.data.gas_allowance_counter,
    ) {
        Ok => Result::Ok((context, code_len_bytes).into()),
        GasExceeded => {
            let system_reservation_ctx =
                SystemReservationContext::from_dispatch(&context.data.dispatch);
            Err(process_error(
                context.data.dispatch,
                context.data.destination_id,
                context.data.gas_counter.burned(),
                system_reservation_ctx,
                ExecutionErrorReason::GasExceeded(GasOperation::ProgramCode),
                false,
            ))
        }
        BlockGasExceeded => Err(process_allowance_exceed(
            context.data.dispatch,
            context.data.destination_id,
            context.data.gas_counter.burned(),
        )),
    }
}

/// Charge a message for instrumentation of the binary code beforehand.
pub fn precharge_for_instrumentation(
    block_config: &BlockConfig,
    mut context: ContextChargedForCode,
    original_code_len_bytes: u32,
) -> PrechargeResult<ContextChargedForInstrumentation> {
    use executor::ChargeForBytesResult::*;

    let cost_base = block_config.module_instrumentation_cost;
    let cost_per_byte = block_config.module_instrumentation_byte_cost;

    let amount =
        cost_base.saturating_add(cost_per_byte.saturating_mul(original_code_len_bytes.into()));
    match executor::charge_gas_for_bytes(
        amount,
        &mut context.data.gas_counter,
        &mut context.data.gas_allowance_counter,
    ) {
        Ok => Result::Ok(context.into()),
        GasExceeded => {
            let system_reservation_ctx =
                SystemReservationContext::from_dispatch(&context.data.dispatch);
            Err(process_error(
                context.data.dispatch,
                context.data.destination_id,
                context.data.gas_counter.burned(),
                system_reservation_ctx,
                ExecutionErrorReason::GasExceeded(GasOperation::ModuleInstrumentation),
                false,
            ))
        }
        BlockGasExceeded => Err(process_allowance_exceed(
            context.data.dispatch,
            context.data.destination_id,
            context.data.gas_counter.burned(),
        )),
    }
}

/// Charge a message for program memory and module instantiation beforehand.
pub fn precharge_for_memory(
    block_config: &BlockConfig,
    mut context: ContextChargedForInstrumentation,
    subsequent_execution: bool,
) -> PrechargeResult<ContextChargedForMemory> {
    let ContextChargedForInstrumentation {
        data:
            ContextData {
                gas_counter,
                gas_allowance_counter,
                actor_data,
                dispatch,
                ..
            },
        code_len_bytes,
    } = &mut context;

    let mut f = || {
        let memory_size = executor::charge_gas_for_pages(
            &block_config.allocations_config,
            gas_counter,
            gas_allowance_counter,
            &actor_data.allocations,
            actor_data.static_pages,
            dispatch.context().is_none() && matches!(dispatch.kind(), DispatchKind::Init),
            subsequent_execution,
        )?;

        executor::charge_gas_for_instantiation(
            block_config.module_instantiation_byte_cost,
            *code_len_bytes,
            gas_counter,
            gas_allowance_counter,
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
                ExecutionErrorReason::BlockGasExceeded(
                    GasOperation::InitialMemory
                    | GasOperation::GrowMemory
                    | GasOperation::LoadMemory
                    | GasOperation::ModuleInstantiation,
                ) => Err(process_allowance_exceed(
                    context.data.dispatch,
                    context.data.destination_id,
                    context.data.gas_counter.burned(),
                )),

                _ => {
                    let system_reservation_ctx =
                        SystemReservationContext::from_dispatch(&context.data.dispatch);
                    Err(process_error(
                        context.data.dispatch,
                        context.data.destination_id,
                        context.data.gas_counter.burned(),
                        system_reservation_ctx,
                        reason,
                        false,
                    ))
                }
            };
        }
    };

    Ok(ContextChargedForMemory {
        data: context.data,
        max_reservations: block_config.max_reservations,
        memory_size,
    })
}

/// Process program & dispatch for it and return journal for updates.
pub fn process<
    A: ProcessorExt + EnvExt + IntoExtInfo<<A as EnvExt>::Error> + 'static,
    E: Environment<A>,
>(
    block_config: &BlockConfig,
    execution_context: ProcessExecutionContext,
    random_data: (Vec<u8>, u32),
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
        random_data,
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
                res.system_reservation_context,
                ExecutionErrorReason::Ext(reason),
                true,
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
            ExecutionErrorReason::BlockGasExceeded(
                GasOperation::InitialMemory | GasOperation::GrowMemory | GasOperation::LoadMemory,
            ) => process_allowance_exceed(dispatch, program_id, e.gas_amount.burned()),
            _ => process_error(
                dispatch,
                program_id,
                e.gas_amount.burned(),
                SystemReservationContext::default(),
                e.reason,
                true,
            ),
        },
    }
}

fn check_is_executable(
    executable_data: Option<ExecutableActorData>,
    dispatch: &IncomingDispatch,
) -> Result<ExecutableActorData, StatusCode> {
    executable_data
        .map(|data| {
            if data.initialized & matches!(dispatch.kind(), DispatchKind::Init) {
                Err(crate::RE_INIT_STATUS_CODE)
            } else {
                Ok(data)
            }
        })
        .unwrap_or(Err(crate::UNAVAILABLE_DEST_STATUS_CODE))
}

/// Helper function for journal creation in trap/error case
fn process_error(
    dispatch: IncomingDispatch,
    program_id: ProgramId,
    gas_burned: u64,
    system_reservation_ctx: SystemReservationContext,
    err: ExecutionErrorReason,
    executed: bool,
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

    if let Some(amount) = system_reservation_ctx.current_reservation {
        journal.push(JournalNote::SystemReserveGas { message_id, amount });
    }

    if system_reservation_ctx.has_any() {
        if !dispatch.is_error_reply()
            && !matches!(dispatch.kind(), DispatchKind::Signal | DispatchKind::Init)
        {
            journal.push(JournalNote::SendSignal {
                message_id,
                destination: program_id,
            });
        }

        journal.push(JournalNote::SystemUnreserveGas { message_id });
    }

    if !dispatch.is_error_reply() && dispatch.kind() != DispatchKind::Signal {
        // This expect panic is unreachable, unless error message is too large or max payload size is too small.
        let err_payload = err.encode().try_into().expect("Error message is too large");
        // # Safety
        //
        // 1. The dispatch.id() has already been checked
        // 2. This reply message is generated by our system
        //
        // So, the message id of this reply message will not be duplicated.
        let dispatch = ReplyMessage::system(dispatch.id(), err_payload, crate::ERR_STATUS_CODE)
            .into_dispatch(program_id, dispatch.source(), dispatch.id());

        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch,
            delay: 0,
            reservation: None,
        });
    }

    let outcome = match dispatch.kind() {
        DispatchKind::Init => DispatchOutcome::InitFailure {
            program_id,
            origin,
            reason: err.to_string(),
            executed,
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
        system_reservation_context,
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
                GasReservationState::Created {
                    amount, duration, ..
                } => Some(JournalNote::ReserveGas {
                    message_id,
                    reservation_id,
                    program_id,
                    amount,
                    duration,
                }),
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

    if let Some(amount) = system_reservation_context.current_reservation {
        journal.push(JournalNote::SystemReserveGas { message_id, amount });
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

    for (dispatch, delay, reservation) in generated_dispatches {
        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch,
            delay,
            reservation,
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

    if system_reservation_context.has_any() {
        journal.push(JournalNote::SystemUnreserveGas { message_id });
    }

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
    status_code: StatusCode,
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
        let dispatch = ReplyMessage::system(dispatch.id(), err_payload, status_code).into_dispatch(
            program_id,
            dispatch.source(),
            dispatch.id(),
        );

        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch,
            delay: 0,
            reservation: None,
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
