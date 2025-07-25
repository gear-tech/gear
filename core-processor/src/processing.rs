// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
    ContextCharged, ForCodeMetadata, ForInstrumentedCode, ForProgram,
    common::{
        ActorExecutionErrorReplyReason, DispatchOutcome, DispatchResult, DispatchResultKind,
        ExecutionError, JournalNote, SuccessfulDispatchResultKind, SystemExecutionError,
        WasmExecutionContext,
    },
    configs::{BlockConfig, ExecutionSettings},
    context::*,
    executor,
    ext::ProcessorExternalities,
};
use alloc::{string::ToString, vec::Vec};
use core::{fmt, fmt::Formatter};
use gear_core::{
    buffer::{Payload, PayloadSizeError},
    env::Externalities,
    ids::{ActorId, MessageId, prelude::*},
    message::{ContextSettings, DispatchKind, IncomingDispatch, ReplyMessage, StoredDispatch},
    reservation::GasReservationState,
};
use gear_core_backend::{
    BackendExternalities,
    error::{BackendAllocSyscallError, BackendSyscallError, RunFallibleError, TrapExplanation},
};
use gear_core_errors::{ErrorReplyReason, SignalCode, SimpleUnavailableActorError};

/// Process program & dispatch for it and return journal for updates.
pub fn process<Ext>(
    block_config: &BlockConfig,
    execution_context: ProcessExecutionContext,
    random_data: (Vec<u8>, u32),
) -> Result<Vec<JournalNote>, SystemExecutionError>
where
    Ext: ProcessorExternalities + BackendExternalities + Send + 'static,
    <Ext as Externalities>::AllocError:
        BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
    RunFallibleError: From<Ext::FallibleError>,
    <Ext as Externalities>::UnrecoverableError: BackendSyscallError,
{
    use crate::common::SuccessfulDispatchResultKind::*;

    let BlockConfig {
        block_info,
        performance_multiplier,
        forbidden_funcs,
        reserve_for,
        gas_multiplier,
        costs,
        existential_deposit,
        mailbox_threshold,
        max_pages,
        outgoing_limit,
        outgoing_bytes_limit,
        ..
    } = block_config.clone();

    let execution_settings = ExecutionSettings {
        block_info,
        performance_multiplier,
        existential_deposit,
        mailbox_threshold,
        max_pages,
        ext_costs: costs.ext,
        lazy_pages_costs: costs.lazy_pages,
        forbidden_funcs,
        reserve_for,
        random_data,
        gas_multiplier,
    };

    let dispatch = execution_context.dispatch;
    let balance = execution_context.balance;
    let program_id = execution_context.program.id;
    let initial_reservations_amount = execution_context.gas_reserver.states().len();

    let execution_context = WasmExecutionContext {
        gas_counter: execution_context.gas_counter,
        gas_allowance_counter: execution_context.gas_allowance_counter,
        gas_reserver: execution_context.gas_reserver,
        program: execution_context.program,
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
    let msg_ctx_settings = ContextSettings {
        sending_fee: costs.db.write.cost_for(2.into()),
        scheduled_sending_fee: costs.db.write.cost_for(4.into()),
        waiting_fee: costs.db.write.cost_for(3.into()),
        waking_fee: costs.db.write.cost_for(2.into()),
        reservation_fee: costs.db.write.cost_for(2.into()),
        outgoing_limit,
        outgoing_bytes_limit,
    };

    // TODO: add tests that system reservation is successfully unreserved after
    // actor execution error #3756.

    // Get system reservation context in order to use it if actor execution error occurs.
    let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);

    let exec_result = executor::execute_wasm::<Ext>(
        balance,
        dispatch.clone(),
        execution_context,
        execution_settings,
        msg_ctx_settings,
    )
    .map_err(|err| {
        log::debug!("Wasm execution error: {err}");
        err
    });

    match exec_result {
        Ok(res) => {
            match res.kind {
                DispatchResultKind::Success
                | DispatchResultKind::Wait(_, _)
                | DispatchResultKind::Exit(_) => {
                    // assert that after processing the initial reservation is less or equal to the current one.
                    // during execution reservation amount might increase due to `system_reserve_gas` calls
                    // thus making initial reservation less than current one.
                    debug_assert!(
                        res.context_store.system_reservation()
                            >= system_reservation_ctx.previous_reservation
                    );
                    debug_assert!(
                        system_reservation_ctx.previous_reservation
                            == res.system_reservation_context.previous_reservation
                    );
                    debug_assert!(
                        res.gas_reserver
                            .as_ref()
                            .map(|reserver| initial_reservations_amount <= reserver.states().len())
                            .unwrap_or(true)
                    );
                }
                // reservation does not change in case of failure
                _ => (),
            }
            Ok(match res.kind {
                DispatchResultKind::Trap(reason) => process_execution_error(
                    dispatch,
                    program_id,
                    res.gas_amount.burned(),
                    res.system_reservation_context,
                    ActorExecutionErrorReplyReason::Trap(reason),
                ),

                DispatchResultKind::Success => process_success(Success, res, dispatch),
                DispatchResultKind::Wait(duration, ref waited_type) => {
                    process_success(Wait(duration, waited_type.clone()), res, dispatch)
                }
                DispatchResultKind::Exit(value_destination) => {
                    process_success(Exit(value_destination), res, dispatch)
                }
                DispatchResultKind::GasAllowanceExceed => {
                    process_allowance_exceed(dispatch, program_id, res.gas_amount.burned())
                }
            })
        }
        Err(ExecutionError::Actor(e)) => Ok(process_execution_error(
            dispatch,
            program_id,
            e.gas_amount.burned(),
            system_reservation_ctx,
            e.reason,
        )),
        Err(ExecutionError::System(e)) => Err(e),
    }
}

enum ProcessErrorCase {
    /// Program exited.
    ProgramExited {
        /// Inheritor of an exited program.
        inheritor: ActorId,
    },
    /// Program failed during init.
    FailedInit,
    /// Program is not initialized yet.
    Uninitialized,
    /// Given code id for program creation doesn't exist.
    CodeNotExists,
    /// Message is executable, but its execution failed due to re-instrumentation.
    ReinstrumentationFailed,
    /// Error is considered as an execution failure.
    ExecutionFailed(ActorExecutionErrorReplyReason),
}

impl fmt::Display for ProcessErrorCase {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ProcessErrorCase::ExecutionFailed(reason) => fmt::Display::fmt(reason, f),
            this => fmt::Display::fmt(&this.to_reason(), f),
        }
    }
}

impl ProcessErrorCase {
    fn to_reason(&self) -> ErrorReplyReason {
        match self {
            ProcessErrorCase::ProgramExited { .. } => {
                ErrorReplyReason::UnavailableActor(SimpleUnavailableActorError::ProgramExited)
            }
            ProcessErrorCase::FailedInit => ErrorReplyReason::UnavailableActor(
                SimpleUnavailableActorError::InitializationFailure,
            ),
            ProcessErrorCase::Uninitialized => {
                ErrorReplyReason::UnavailableActor(SimpleUnavailableActorError::Uninitialized)
            }
            ProcessErrorCase::CodeNotExists => {
                ErrorReplyReason::UnavailableActor(SimpleUnavailableActorError::ProgramNotCreated)
            }
            ProcessErrorCase::ReinstrumentationFailed => ErrorReplyReason::UnavailableActor(
                SimpleUnavailableActorError::ReinstrumentationFailure,
            ),
            ProcessErrorCase::ExecutionFailed(reason) => reason.as_simple().into(),
        }
    }

    // TODO: consider to convert `self` into `Payload` to avoid `PanicBuffer` cloning (#4594)
    fn to_payload(&self) -> Payload {
        match self {
            ProcessErrorCase::ProgramExited { inheritor } => {
                const _: () = assert!(size_of::<ActorId>() <= Payload::MAX_LEN);
                inheritor
                    .into_bytes()
                    .to_vec()
                    .try_into()
                    .unwrap_or_else(|PayloadSizeError| {
                        unreachable!("`ActorId` is always smaller than maximum payload size")
                    })
            }
            ProcessErrorCase::ExecutionFailed(ActorExecutionErrorReplyReason::Trap(
                TrapExplanation::Panic(buf),
            )) => buf.inner().clone(),
            _ => Payload::default(),
        }
    }
}

fn process_error(
    dispatch: IncomingDispatch,
    program_id: ActorId,
    gas_burned: u64,
    system_reservation_ctx: SystemReservationContext,
    case: ProcessErrorCase,
) -> Vec<JournalNote> {
    let mut journal = Vec::new();

    let message_id = dispatch.id();
    let origin = dispatch.source();
    let value = dispatch.value();

    journal.push(JournalNote::GasBurned {
        message_id,
        amount: gas_burned,
    });

    let to_send_reply = !matches!(dispatch.kind(), DispatchKind::Reply | DispatchKind::Signal);

    // We check if value is greater than zero to don't provide
    // no-op journal note.
    //
    // We also check if dispatch had context of previous executions:
    // it's existence shows that we have processed message after
    // being waken, so the value were already transferred in
    // execution, where `gr_wait` was called.
    if dispatch.context().is_none() && value != 0 {
        // Value on error is always delivered to the program, but may return with error reply.
        journal.push(JournalNote::SendValue {
            from: origin,
            to: program_id,
            value,
            // in case of upcoming error reply, we want to send locked value,
            // instead of deposit, to avoid ED manipulations.
            locked: to_send_reply,
        });
    }

    if let Some(amount) = system_reservation_ctx.current_reservation {
        journal.push(JournalNote::SystemReserveGas { message_id, amount });
    }

    if let ProcessErrorCase::ExecutionFailed(reason) = &case {
        // TODO: consider to handle error reply and init #3701
        if system_reservation_ctx.has_any()
            && !dispatch.is_error_reply()
            && !matches!(dispatch.kind(), DispatchKind::Signal | DispatchKind::Init)
        {
            journal.push(JournalNote::SendSignal {
                message_id,
                destination: program_id,
                code: SignalCode::Execution(reason.as_simple()),
            });
        }
    }

    if system_reservation_ctx.has_any() {
        journal.push(JournalNote::SystemUnreserveGas { message_id });
    }

    if to_send_reply {
        let err = case.to_reason();
        let err_payload = case.to_payload();

        let value = if dispatch.context().is_none() {
            value
        } else {
            0
        };

        // # Safety
        //
        // 1. The dispatch.id() has already been checked
        // 2. This reply message is generated by our system
        //
        // So, the message id of this reply message will not be duplicated.
        let dispatch = ReplyMessage::system(dispatch.id(), err_payload, value, err).into_dispatch(
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

    let outcome = match case {
        ProcessErrorCase::ExecutionFailed { .. } | ProcessErrorCase::ReinstrumentationFailed => {
            let err_msg = case.to_string();
            match dispatch.kind() {
                DispatchKind::Init => DispatchOutcome::InitFailure {
                    program_id,
                    origin,
                    reason: err_msg,
                },
                _ => DispatchOutcome::MessageTrap {
                    program_id,
                    trap: err_msg,
                },
            }
        }
        ProcessErrorCase::ProgramExited { .. }
        | ProcessErrorCase::FailedInit
        | ProcessErrorCase::Uninitialized
        | ProcessErrorCase::CodeNotExists => DispatchOutcome::NoExecution,
    };

    journal.push(JournalNote::MessageDispatched {
        message_id,
        source: origin,
        outcome,
    });
    journal.push(JournalNote::MessageConsumed(message_id));

    journal
}

/// Helper function for journal creation in trap/error case.
pub fn process_execution_error(
    dispatch: IncomingDispatch,
    program_id: ActorId,
    gas_burned: u64,
    system_reservation_ctx: SystemReservationContext,
    err: impl Into<ActorExecutionErrorReplyReason>,
) -> Vec<JournalNote> {
    process_error(
        dispatch,
        program_id,
        gas_burned,
        system_reservation_ctx,
        ProcessErrorCase::ExecutionFailed(err.into()),
    )
}

/// Helper function for journal creation in program exited case.
pub fn process_program_exited(
    context: ContextCharged<ForProgram>,
    inheritor: ActorId,
) -> Vec<JournalNote> {
    let (destination_id, dispatch, gas_counter, _) = context.into_parts();

    let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);

    process_error(
        dispatch,
        destination_id,
        gas_counter.burned(),
        system_reservation_ctx,
        ProcessErrorCase::ProgramExited { inheritor },
    )
}

/// Helper function for journal creation in program failed init case.
pub fn process_failed_init(context: ContextCharged<ForProgram>) -> Vec<JournalNote> {
    let (destination_id, dispatch, gas_counter, _) = context.into_parts();

    let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);

    process_error(
        dispatch,
        destination_id,
        gas_counter.burned(),
        system_reservation_ctx,
        ProcessErrorCase::FailedInit,
    )
}

/// Helper function for journal creation in program uninitialized case.
pub fn process_uninitialized(context: ContextCharged<ForProgram>) -> Vec<JournalNote> {
    let (destination_id, dispatch, gas_counter, _) = context.into_parts();

    let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);

    process_error(
        dispatch,
        destination_id,
        gas_counter.burned(),
        system_reservation_ctx,
        ProcessErrorCase::Uninitialized,
    )
}

/// Helper function for journal creation in code not exists case.
pub fn process_code_not_exists(context: ContextCharged<ForProgram>) -> Vec<JournalNote> {
    let (destination_id, dispatch, gas_counter, _) = context.into_parts();

    let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);

    process_error(
        dispatch,
        destination_id,
        gas_counter.burned(),
        system_reservation_ctx,
        ProcessErrorCase::CodeNotExists,
    )
}

/// Helper function for journal creation in case of re-instrumentation error.
pub fn process_reinstrumentation_error(
    context: ContextCharged<ForInstrumentedCode>,
) -> Vec<JournalNote> {
    let (destination_id, dispatch, gas_counter, _) = context.into_parts();

    let gas_burned = gas_counter.burned();
    let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);

    process_error(
        dispatch,
        destination_id,
        gas_burned,
        system_reservation_ctx,
        ProcessErrorCase::ReinstrumentationFailed,
    )
}

/// Helper function for journal creation in case of instrumentation failure.
pub fn process_instrumentation_failed(
    context: ContextCharged<ForCodeMetadata>,
) -> Vec<JournalNote> {
    let (destination_id, dispatch, gas_counter, _) = context.into_parts();

    let gas_burned = gas_counter.burned();
    let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);

    process_error(
        dispatch,
        destination_id,
        gas_burned,
        system_reservation_ctx,
        ProcessErrorCase::ReinstrumentationFailed,
    )
}

/// Helper function for journal creation in success case
pub fn process_success(
    kind: SuccessfulDispatchResultKind,
    dispatch_result: DispatchResult,
    dispatch: IncomingDispatch,
) -> Vec<JournalNote> {
    use crate::common::SuccessfulDispatchResultKind::*;

    let DispatchResult {
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
        reply_deposits,
        reply_sent,
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
            to: program_id,
            value,
            locked: false,
        });
    }

    // Must be handled before handling generated dispatches.
    for (code_id, candidates) in program_candidates {
        journal.push(JournalNote::StoreNewPrograms {
            program_id,
            code_id,
            candidates,
        });
    }

    // Sending auto-generated reply about success execution.
    if !matches!(kind, SuccessfulDispatchResultKind::Wait(_, _))
        && !matches!(dispatch.kind(), DispatchKind::Reply | DispatchKind::Signal)
        && !reply_sent
    {
        let auto_reply = ReplyMessage::auto(dispatch.id()).into_dispatch(
            program_id,
            dispatch.source(),
            dispatch.id(),
        );

        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch: auto_reply,
            delay: 0,
            reservation: None,
        });
    }

    for (message_id_sent, amount) in reply_deposits {
        journal.push(JournalNote::ReplyDeposit {
            message_id,
            future_reply_id: MessageId::generate_reply(message_id_sent),
            amount,
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

/// Helper function for journal creation if the block gas allowance has been exceeded.
pub fn process_allowance_exceed(
    dispatch: IncomingDispatch,
    program_id: ActorId,
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
