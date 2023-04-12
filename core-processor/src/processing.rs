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
        ActorExecutionErrorReason, DispatchOutcome, DispatchResult, DispatchResultKind,
        ExecutionError, JournalNote, SystemExecutionError, WasmExecutionContext,
    },
    configs::{BlockConfig, ExecutionSettings},
    context::*,
    executor,
    ext::ProcessorExt,
    precharge::SuccessfulDispatchResultKind,
};
use alloc::{collections::BTreeMap, string::ToString, vec::Vec};
use gear_backend_common::{BackendExt, BackendExtError, Environment, SystemReservationContext};
use gear_core::{
    env::Ext,
    ids::ProgramId,
    memory::{GearPage, PageBuf},
    message::{ContextSettings, DispatchKind, IncomingDispatch, ReplyMessage, StoredDispatch},
    reservation::GasReservationState,
};
use gear_core_errors::{SimpleReplyError, SimpleSignalError};

/// Process program & dispatch for it and return journal for updates.
pub fn process<E>(
    block_config: &BlockConfig,
    execution_context: ProcessExecutionContext,
    random_data: (Vec<u8>, u32),
    memory_pages: BTreeMap<GearPage, PageBuf>,
) -> Result<Vec<JournalNote>, SystemExecutionError>
where
    E: Environment,
    E::Ext: ProcessorExt + BackendExt + 'static,
    <E::Ext as Ext>::Error: BackendExtError,
{
    use crate::precharge::SuccessfulDispatchResultKind::*;

    let BlockConfig {
        block_info,
        max_pages,
        page_costs,
        existential_deposit,
        outgoing_limit,
        host_fn_weights,
        forbidden_funcs,
        mailbox_threshold,
        waitlist_cost,
        dispatch_hold_cost,
        reserve_for,
        reservation,
        write_cost,
        ..
    } = block_config.clone();

    let execution_settings = ExecutionSettings {
        block_info,
        existential_deposit,
        max_pages,
        page_costs,
        host_fn_weights,
        forbidden_funcs,
        mailbox_threshold,
        waitlist_cost,
        dispatch_hold_cost,
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

    let exec_result = executor::execute_wasm::<E>(
        balance,
        dispatch.clone(),
        execution_context,
        execution_settings,
        msg_ctx_settings,
    )
    .map_err(|err| {
        log::debug!("Wasm execution error: {}", err);
        err
    });

    match exec_result {
        Ok(res) => Ok(match res.kind {
            DispatchResultKind::Trap(reason) => process_error(
                res.dispatch,
                program_id,
                res.gas_amount.burned(),
                res.system_reservation_context,
                ActorExecutionErrorReason::Trap(reason),
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
        }),
        Err(ExecutionError::Actor(e)) => Ok(process_error(
            dispatch,
            program_id,
            e.gas_amount.burned(),
            SystemReservationContext::default(),
            e.reason,
            true,
        )),
        Err(ExecutionError::System(e)) => Err(e),
    }
}

/// Helper function for journal creation in trap/error case
pub fn process_error(
    dispatch: IncomingDispatch,
    program_id: ProgramId,
    gas_burned: u64,
    system_reservation_ctx: SystemReservationContext,
    err: ActorExecutionErrorReason,
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
                err: SimpleSignalError::Execution(err.as_simple()),
            });
        }

        journal.push(JournalNote::SystemUnreserveGas { message_id });
    }

    if !dispatch.is_error_reply() && dispatch.kind() != DispatchKind::Signal {
        // This expect panic is unreachable, unless error message is too large or max payload size is too small.
        let err_payload = err
            .to_string()
            .into_bytes()
            .try_into()
            .unwrap_or_else(|_| unreachable!("Error message is too large"));
        let err = SimpleReplyError::Execution(err.as_simple());

        // # Safety
        //
        // 1. The dispatch.id() has already been checked
        // 2. This reply message is generated by our system
        //
        // So, the message id of this reply message will not be duplicated.
        let dispatch = ReplyMessage::system(dispatch.id(), err_payload, err).into_dispatch(
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
pub fn process_success(
    kind: SuccessfulDispatchResultKind,
    dispatch_result: DispatchResult,
) -> Vec<JournalNote> {
    use crate::precharge::SuccessfulDispatchResultKind::*;

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

    // Sending auto-generated reply about success execution.
    if matches!(kind, SuccessfulDispatchResultKind::Success)
        && !context_store.reply_sent()
        && !dispatch.is_reply()
        && dispatch.kind() != DispatchKind::Signal
    {
        let dispatch = ReplyMessage::auto(dispatch.id()).into_dispatch(
            program_id,
            dispatch.source(),
            dispatch.id(),
        );

        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch,
            delay: 0,
            reservation: None,
        })
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

    if !allocations.is_empty() {
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

pub fn process_allowance_exceed(
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
pub fn process_non_executable(
    dispatch: IncomingDispatch,
    program_id: ProgramId,
    system_reservation_ctx: SystemReservationContext,
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
        let err = SimpleReplyError::NonExecutable;
        let err_payload = err
            .to_string()
            .into_bytes()
            .try_into()
            .unwrap_or_else(|_| unreachable!("Error message is too large"));
        // # Safety
        //
        // 1. The dispatch.id() has already been checked
        // 2. This reply message is generated by our system
        //
        // So, the message id of this reply message will not be duplicated.
        let dispatch = ReplyMessage::system(dispatch.id(), err_payload, err).into_dispatch(
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

    if system_reservation_ctx.has_any() {
        journal.push(JournalNote::SystemUnreserveGas { message_id });
    }

    journal.push(JournalNote::MessageDispatched {
        message_id,
        source,
        outcome: DispatchOutcome::NoExecution,
    });

    journal.push(JournalNote::MessageConsumed(message_id));

    journal
}
