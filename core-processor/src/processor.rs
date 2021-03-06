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
use alloc::{string::ToString, vec::Vec};
use codec::Encode;
use gear_backend_common::{Environment, IntoExtInfo};
use gear_core::{
    env::Ext as EnvExt,
    ids::{MessageId, ProgramId},
    message::{
        DispatchKind, ExitCode, IncomingDispatch, ReplyMessage, ReplyPacket, StoredDispatch,
    },
};

enum SuccessfulDispatchResultKind {
    Exit(ProgramId),
    Wait,
    Success,
}

/// Process program & dispatch for it and return journal for updates.
pub fn process<A: ProcessorExt + EnvExt + IntoExtInfo + 'static, E: Environment<A>>(
    block_config: &BlockConfig,
    execution_context: MessageExecutionContext,
) -> Vec<JournalNote> {
    let MessageExecutionContext {
        actor,
        dispatch,
        origin,
        gas_allowance,
    } = execution_context;
    let Actor {
        balance,
        destination_program,
        executable_data,
    } = actor;

    match check_is_executable(executable_data, &dispatch) {
        Err(exit_code) => process_non_executable(dispatch, destination_program, exit_code),
        Ok(data) => process_executable::<A, E>(
            origin,
            gas_allowance,
            data,
            dispatch,
            balance,
            block_config.clone(),
        ),
    }
}

fn check_is_executable(
    executable_data: Option<ExecutableActorData>,
    dispatch: &IncomingDispatch,
) -> Result<ExecutableActorData, ExitCode> {
    executable_data
        .map(|data| {
            if data.program.is_initialized() & matches!(dispatch.kind(), DispatchKind::Init) {
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

    if !dispatch.is_reply() || dispatch.exit_code().expect("Checked before") == 0 {
        let id = MessageId::generate_reply(dispatch.id(), crate::ERR_EXIT_CODE);
        let packet = ReplyPacket::system(err.encode(), crate::ERR_EXIT_CODE);
        let message = ReplyMessage::from_packet(id, packet);

        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch: message.into_dispatch(program_id, dispatch.source(), dispatch.id()),
        });
    }

    let outcome = match dispatch.kind() {
        DispatchKind::Init => DispatchOutcome::InitFailure {
            program_id,
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
        Wait => {
            journal.push(JournalNote::WaitDispatch(
                dispatch.into_stored(program_id, context_store),
            ));

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

pub fn process_executable<A: ProcessorExt + EnvExt + IntoExtInfo + 'static, E: Environment<A>>(
    origin: ProgramId,
    gas_allowance: u64,
    data: ExecutableActorData,
    dispatch: IncomingDispatch,
    balance: u128,
    block_config: BlockConfig,
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
    } = block_config;

    let execution_settings = ExecutionSettings::new(
        block_info,
        existential_deposit,
        allocations_config,
        host_fn_weights,
        forbidden_funcs,
        mailbox_threshold,
    );
    let execution_context = WasmExecutionContext {
        origin,
        gas_allowance,
    };
    let msg_ctx_settings = gear_core::message::ContextSettings::new(0, outgoing_limit);

    let program_id = data.program.id();

    let exec_result = executor::execute_wasm::<A, E>(
        balance,
        data,
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
            DispatchResultKind::Wait => process_success(Wait, res),
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
    if !dispatch.is_reply() || dispatch.exit_code().expect("Checked before") == 0 {
        let id = MessageId::generate_reply(dispatch.id(), exit_code);
        let packet = ReplyPacket::system(ExecutionErrorReason::NonExecutable.encode(), exit_code);
        let message = ReplyMessage::from_packet(id, packet);

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
