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
use alloc::{collections::BTreeMap, vec::Vec};
use gear_backend_common::{Environment, IntoExtInfo};
use gear_core::{
    env::Ext as EnvExt,
    message::{Dispatch, DispatchKind, ExitCode, Message},
    program::ProgramId,
};

enum SuccessfulDispatchResultKind {
    Exit(ProgramId),
    Wait,
    Success,
}

/// Process program & dispatch for it and return journal for updates.
pub fn process<A: ProcessorExt + EnvExt + IntoExtInfo + 'static, E: Environment<A>>(
    maybe_actor: Option<ExecutableActor>,
    dispatch: Dispatch,
    block_info: BlockInfo,
    existential_deposit: u128,
    origin: ProgramId,
) -> Vec<JournalNote> {
    match check_is_executable(maybe_actor, &dispatch) {
        Err(exit_code) => process_non_executable(dispatch, exit_code),
        Ok(actor) => {
            process_executable::<A, E>(actor, dispatch, block_info, existential_deposit, origin)
        }
    }
}

/// Process multiple dispatches into multiple programs and return journal notes for update.
pub fn process_many<A: ProcessorExt + EnvExt + IntoExtInfo + 'static, E: Environment<A>>(
    mut actors: BTreeMap<ProgramId, Option<ExecutableActor>>,
    dispatches: Vec<Dispatch>,
    block_info: BlockInfo,
    existential_deposit: u128,
    // Will go away some time soon
    origins: Vec<ProgramId>,
) -> Vec<JournalNote> {
    let mut journal = Vec::new();

    assert_eq!(dispatches.len(), origins.len());

    for (dispatch, origin) in dispatches.into_iter().zip(origins.into_iter()) {
        let actor = actors
            .get_mut(&dispatch.message.dest())
            .expect("Program wasn't found in programs");

        let current_journal = process::<A, E>(
            actor.clone(),
            dispatch,
            block_info,
            existential_deposit,
            origin,
        );

        for note in &current_journal {
            if let Some(actor) = actor {
                match note {
                    JournalNote::UpdateNonce { nonce, .. } => {
                        actor.program.set_message_nonce(*nonce)
                    }
                    JournalNote::UpdatePage {
                        page_number, data, ..
                    } => {
                        if let Some(data) = data {
                            actor
                                .program
                                .set_page(*page_number, data)
                                .expect("Can't fail");
                        } else {
                            actor.program.remove_page(*page_number);
                        }
                    }
                    _ => {}
                }
            }
        }

        journal.extend(current_journal);
    }

    journal
}

fn check_is_executable(
    maybe_actor: Option<ExecutableActor>,
    dispatch: &Dispatch,
) -> Result<ExecutableActor, ExitCode> {
    maybe_actor
        .map(|a| {
            if a.program.is_initialized() & matches!(dispatch.kind, DispatchKind::Init) {
                Err(crate::RE_INIT_EXIT_CODE)
            } else {
                Ok(a)
            }
        })
        .unwrap_or(Err(crate::UNAVAILABLE_DEST_EXIT_CODE))
}

/// Helper function for journal creation in trap/error case
fn process_error(
    dispatch: Dispatch,
    initial_nonce: u64,
    gas_burned: u64,
    err: Option<&'static str>,
) -> Vec<JournalNote> {
    let mut journal = Vec::new();

    let Dispatch { kind, message, .. } = dispatch;
    let message_id = message.id();
    let origin = message.source();
    let program_id = message.dest();
    let value = message.value();

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

    if let Some(message) = generate_trap_reply(&message, initial_nonce) {
        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch: Dispatch::new_reply(message),
        });
        journal.push(JournalNote::UpdateNonce {
            program_id,
            nonce: initial_nonce + 1,
        });
    }

    let outcome = match kind {
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
fn process_success(
    kind: SuccessfulDispatchResultKind,
    DispatchResult {
        dispatch,
        generated_dispatches,
        awakening,
        program_candidates,
        gas_amount,
        page_update,
        nonce,
        ..
    }: DispatchResult,
) -> Vec<JournalNote> {
    use SuccessfulDispatchResultKind::*;

    let mut journal = Vec::new();

    let message_id = dispatch.message.id();
    let origin = dispatch.message.source();
    let program_id = dispatch.message.dest();
    let value = dispatch.message.value();

    journal.push(JournalNote::GasBurned {
        message_id,
        amount: gas_amount.burned(),
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

    journal.push(JournalNote::UpdateNonce { program_id, nonce });

    for (page_number, data) in page_update {
        journal.push(JournalNote::UpdatePage {
            program_id,
            page_number,
            data,
        })
    }

    match kind {
        Exit(value_destination) => {
            journal.push(JournalNote::ExitDispatch {
                id_exited: program_id,
                value_destination,
            });
        }
        Wait => {
            journal.push(JournalNote::WaitDispatch(dispatch));
        }
        Success => {
            let outcome = match dispatch.kind {
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
    };

    journal
}

pub fn process_executable<A: ProcessorExt + EnvExt + IntoExtInfo + 'static, E: Environment<A>>(
    actor: ExecutableActor,
    dispatch: Dispatch,
    block_info: BlockInfo,
    existential_deposit: u128,
    origin: ProgramId,
) -> Vec<JournalNote> {
    use SuccessfulDispatchResultKind::*;

    let execution_settings = ExecutionSettings::new(block_info, existential_deposit);
    let execution_context = ExecutionContext { origin };
    let initial_nonce = actor.program.message_nonce();

    match executor::execute_wasm::<A, E>(
        actor,
        dispatch.clone(),
        execution_context,
        execution_settings,
    ) {
        Ok(res) => match res.kind {
            DispatchResultKind::Trap(reason) => {
                process_error(res.dispatch, initial_nonce, res.gas_amount.burned(), reason)
            }
            DispatchResultKind::Success => process_success(Success, res),
            DispatchResultKind::Wait => process_success(Wait, res),
            DispatchResultKind::Exit(value_destination) => {
                process_success(Exit(value_destination), res)
            }
        },
        Err(e) => process_error(
            dispatch,
            initial_nonce,
            e.gas_amount.burned(),
            Some(e.reason),
        ),
    }
}

/// Helper function for journal creation in message no execution case
fn process_non_executable(dispatch: Dispatch, exit_code: ExitCode) -> Vec<JournalNote> {
    // Number of notes is predetermined
    let mut journal = Vec::with_capacity(4);

    let Dispatch { message, .. } = dispatch;

    let message_id = message.id();
    let value = message.value();

    if value != 0 {
        // Send value back
        journal.push(JournalNote::SendValue {
            from: message.source(),
            to: None,
            value,
        });
    }

    // Reply back to the message `source`
    let reply_message = Message::new_reply(
        crate::id::next_system_reply_message_id(message.dest(), message_id),
        message.dest(),
        message.source(),
        Default::default(),
        // Error reply value must be 0!
        0,
        message_id,
        exit_code,
    );
    journal.push(JournalNote::SendDispatch {
        message_id,
        dispatch: Dispatch::new_reply(reply_message),
    });
    journal.push(JournalNote::MessageDispatched(
        DispatchOutcome::NoExecution(message_id),
    ));
    journal.push(JournalNote::MessageConsumed(message_id));

    journal
}

/// Helper function for reply generation
fn generate_trap_reply(message: &Message, nonce: u64) -> Option<Message> {
    if let Some((_, exit_code)) = message.reply() {
        if exit_code != 0 {
            return None;
        }
    };

    let new_message_id = crate::id::next_message_id(message.dest(), nonce);

    Some(Message::new_reply(
        new_message_id,
        message.dest(),
        message.source(),
        Default::default(),
        0,
        message.id(),
        crate::ERR_EXIT_CODE,
    ))
}
