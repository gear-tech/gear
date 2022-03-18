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
        DispatchOutcome, DispatchResult, DispatchResultKind, ExecutableActor, ExecutionContext,
        JournalNote,
    },
    configs::{BlockInfo, ExecutionSettings},
    executor,
    ext::ProcessorExt,
};
use alloc::vec::Vec;
use gear_backend_common::Environment;
use gear_backend_common::ExtInfo;
use gear_core::{
    env::Ext as EnvExt,
    identifiers::{MessageId, ProgramId},
    message::{DispatchKind, ExitCode, IncomingDispatch, ReplyMessage, ReplyPacket},
};

/// Process program & dispatch for it and return journal for updates.
pub fn process<A: ProcessorExt + EnvExt + Into<ExtInfo> + 'static, E: Environment<A>>(
    actor: Option<ExecutableActor>,
    dispatch: IncomingDispatch,
    block_info: BlockInfo,
    existential_deposit: u128,
    origin: ProgramId,
    // TODO: Temporary here for non-executable case. Should be inside executable actor, renamed to Actor.
    program_id: ProgramId,
) -> Vec<JournalNote> {
    if let Err(exit_code) = check_is_executable(actor.as_ref(), &dispatch) {
        process_non_executable(dispatch, program_id, exit_code)
    } else {
        let actor = actor.expect("message is not executed if actor is none");
        let program_id = actor.program.id();
        let execution_settings = ExecutionSettings::new(block_info, existential_deposit);
        let execution_context = ExecutionContext { origin };

        match executor::execute_wasm::<A, E>(
            actor,
            dispatch.clone(),
            execution_context,
            execution_settings,
        ) {
            Ok(res) => match res.kind {
                DispatchResultKind::Trap(reason) => process_error(
                    res.dispatch,
                    res.program_id,
                    res.gas_amount.burned(),
                    reason,
                ),
                _ => process_success(res),
            },
            Err(e) => process_error(dispatch, program_id, e.gas_amount.burned(), Some(e.reason)),
        }
    }
}

fn check_is_executable(
    actor: Option<&ExecutableActor>,
    dispatch: &IncomingDispatch,
) -> Result<(), ExitCode> {
    match actor.map(|a| a.program.is_initialized()) {
        Some(true) if matches!(dispatch.kind(), DispatchKind::Init) => {
            Err(crate::RE_INIT_EXIT_CODE)
        }
        None => Err(crate::UNAVAILABLE_DEST_EXIT_CODE),
        _ => Ok(()),
    }
}

/// Helper function for journal creation in trap/error case
fn process_error(
    dispatch: IncomingDispatch,
    program_id: ProgramId,
    gas_burned: u64,
    err: Option<&'static str>,
) -> Vec<JournalNote> {
    let mut journal = Vec::new();

    let message_id = dispatch.id();
    let origin = dispatch.source();
    let value = dispatch.value();

    journal.push(JournalNote::GasBurned {
        message_id,
        amount: gas_burned,
    });

    if value != 0 {
        // Send back value
        journal.push(JournalNote::SendValue {
            from: origin,
            to: None,
            value,
        });
    }

    if !dispatch.is_reply() || dispatch.exit_code().expect("Checked before") == 0 {
        let id = MessageId::generate_reply(dispatch.id(), crate::ERR_EXIT_CODE);
        let packet = ReplyPacket::system(crate::ERR_EXIT_CODE);
        let message = ReplyMessage::from_packet(id, packet);

        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch: message.into_dispatch(program_id, dispatch.source(), dispatch.id()),
        });
    }

    let outcome = match dispatch.kind() {
        DispatchKind::Init => DispatchOutcome::InitFailure {
            message_id,
            origin,
            program_id,
            reason: err.unwrap_or_default(),
        },
        _ => DispatchOutcome::MessageTrap {
            message_id,
            program_id,
            trap: err,
        },
    };

    journal.push(JournalNote::MessageDispatched(outcome));
    journal.push(JournalNote::MessageConsumed(message_id));

    journal
}

/// Helper function for journal creation in success case
fn process_success(res: DispatchResult) -> Vec<JournalNote> {
    let mut journal = Vec::new();

    let message_id = res.message_id();
    let origin = res.message_source();
    let program_id = res.program_id();
    let value = res.message_value();

    journal.push(JournalNote::GasBurned {
        message_id,
        amount: res.gas_amount.burned(),
    });

    if value != 0 {
        // Send value further
        journal.push(JournalNote::SendValue {
            from: origin,
            to: Some(program_id),
            value,
        });
    }

    // Must be handled before handling generated dispatches.
    for (code_hash, candidates) in res.program_candidates {
        journal.push(JournalNote::StoreNewPrograms {
            code_hash,
            candidates,
        });
    }

    for dispatch in res.generated_dispatches {
        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch,
        });
    }

    for awakening_id in res.awakening {
        journal.push(JournalNote::WakeMessage {
            message_id,
            program_id,
            awakening_id,
        });
    }

    for (page_number, data) in res.page_update {
        journal.push(JournalNote::UpdatePage {
            program_id,
            page_number,
            data,
        })
    }

    match res.kind {
        DispatchResultKind::Exit(value_destination) => {
            journal.push(JournalNote::ExitDispatch {
                id_exited: program_id,
                value_destination,
            });
        }
        DispatchResultKind::Wait => {
            journal.push(JournalNote::WaitDispatch(
                res.dispatch.into_stored(program_id, res.context_store),
            ));
        }
        DispatchResultKind::Success => {
            let outcome = match res.dispatch.kind() {
                DispatchKind::Init => DispatchOutcome::InitSuccess {
                    message_id,
                    origin,
                    program_id,
                },
                _ => DispatchOutcome::Success(message_id),
            };

            journal.push(JournalNote::MessageDispatched(outcome));
            journal.push(JournalNote::MessageConsumed(message_id));
        }
        // Handled in other function
        _ => {
            unreachable!()
        }
    };

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
        let packet = ReplyPacket::system(exit_code);
        let message = ReplyMessage::from_packet(id, packet);

        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch: message.into_dispatch(program_id, dispatch.source(), dispatch.id()),
        });
    }

    journal.push(JournalNote::MessageDispatched(
        DispatchOutcome::NoExecution(message_id),
    ));

    journal.push(JournalNote::MessageConsumed(message_id));

    journal
}
