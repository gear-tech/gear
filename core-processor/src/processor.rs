// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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
        DispatchOutcome, DispatchResultKind, JournalNote, SendValueNoteFactory,
    },
    configs::{BlockInfo, ExecutionSettings},
    executor,
    ext::Ext,
};
use alloc::{collections::BTreeMap, vec::Vec};
use gear_backend_common::Environment;
use gear_core::{
    message::{Message, Dispatch, DispatchKind},
    program::{Program, ProgramId},
};

/// Process program & dispatch for it and return journal for updates.
pub fn process<E: Environment<Ext>>(
    program: Option<Program>,
    dispatch: Dispatch,
    block_info: BlockInfo,
) -> Vec<JournalNote> {
    let execution_settings = ExecutionSettings::new(block_info);
    let initial_nonce = program.message_nonce();

    match executor::execute_wasm::<E>(program, dispatch.clone(), execution_settings) {
        Ok(res) => match res.kind {
            DispatchResultKind::Trap(reason) => {
                process_error(res.dispatch, initial_nonce, res.gas_amount.burned(), reason)
            }
            _ => process_success(res),
        },
        Err(e) => process_error(
            dispatch,
            initial_nonce,
            e.gas_amount.burned(),
            Some(e.reason),
        ),
    }
}

/// Process multiple dispatches into multiple programs and return journal notes for update.
pub fn process_many<E: Environment<Ext>>(
    mut programs: BTreeMap<ProgramId, Program>,
    dispatches: Vec<Dispatch>,
    block_info: BlockInfo,
) -> Vec<JournalNote> {
    let mut journal = Vec::new();

    for dispatch in dispatches {
        let program = programs
            .get_mut(&dispatch.message.dest())
            .expect("Program wasn't found in programs");

        // todo [sab] TMP FIX
        let current_journal = process::<E>(Some(program.clone()), dispatch, block_info);

        for note in &current_journal {
            match note {
                JournalNote::UpdateNonce { nonce, .. } => program.set_message_nonce(*nonce),
                JournalNote::UpdatePage {
                    page_number, data, ..
                } => {
                    if let Some(data) = data {
                        program.set_page(*page_number, data).expect("Can't fail");
                    } else {
                        program.remove_page(*page_number);
                    }
                }
                _ => continue,
            }
        }

        journal.extend(current_journal);
    }

    journal
}

/// Helper function for reply generation
/// todo [sab] нужно единообразие в системных reply сообщеиях тут и в скипе
fn generate_trap_reply(message: &Message, gas_limit: u64, nonce: u64) -> Option<Message> {
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
        gas_limit,
        0,
        message.id(),
        crate::ERR_EXIT_CODE,
    ))
}

/// Helper function for journal creation in message skip case
fn process_skip(dispatch: Dispatch) -> Vec<JournalNote> {
    // Number of notes is predetermined
    let mut journal = Vec::with_capacity(4);

    let message_id = dispatch.message.id();
    let origin = dispatch.message.source();
    let value = dispatch.message.value();

    // Reply back to the message `origin`
    let reply_message = Message::new_reply(
        crate::id::next_system_reply_message_id(origin, message_id),
        dispatch.message.dest(),
        origin,
        Default::default(),
        dispatch.message.gas_limit(),
        // must be 0!
        0,
        message_id,
        crate::ERR_EXIT_CODE,
    );
    journal.push(JournalNote::SendDispatch {
        message_id,
        dispatch: Dispatch {
            kind: DispatchKind::HandleReply,
            message: reply_message,
        }
    });

    journal.push(JournalNote::MessageDispatched(
        DispatchOutcome::Skip(message_id),
    ));

    journal.push(JournalNote::MessageConsumed(message_id));

    if value != 0 {
        // Send back value
        journal.push(JournalNote::SendValue {
            from: origin,
            to: None,
            value
        });
    }

    journal
}

/// Helper function for journal creation in trap/error case
fn process_error(
    dispatch: Dispatch,
    initial_nonce: u64,
    gas_burned: u64,
    err: Option<&'static str>,
) -> Vec<JournalNote> {
    let mut journal = Vec::new();

    let Dispatch { kind, message } = dispatch;
    let message_id = message.id();
    let origin = message.source();
    let program_id = message.dest();
    let gas_left = message.gas_limit() - gas_burned;
    let value = dispatch.message.value();

    journal.push(JournalNote::GasBurned {
        message_id,
        origin,
        amount: gas_burned,
    });

    if let Some(message) = generate_trap_reply(&message, gas_left, initial_nonce) {
        journal.push(JournalNote::SendDispatch {
            message_id,
            dispatch: Dispatch {
                kind: DispatchKind::HandleReply,
                message,
            },
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

    if value != 0 {
        // Send back value
        journal.push(JournalNote::SendValue {
            from: origin,
            to: None,
            value
        });
    }

    journal
}

/// Helper function for journal creation in success case
fn process_success(res: DispatchResult) -> Vec<JournalNote> {
    let mut journal = Vec::new();

    let message_id = res.message_id();
    let origin = res.message_source();
    let program_id = res.program_id();

    journal.push(JournalNote::GasBurned {
        message_id,
        origin,
        amount: res.gas_amount.burned(),
    });

    for dispatch in res.outgoing {
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

    journal.push(JournalNote::UpdateNonce {
        program_id,
        nonce: res.nonce,
    });

    for (page_number, data) in res.page_update {
        journal.push(JournalNote::UpdatePage {
            program_id,
            page_number,
            data,
        })
    }

    match res.kind {
        DispatchResultKind::Wait => {
            let mut dispatch = res.dispatch;
            dispatch.message.gas_limit = res.gas_amount.left();

            journal.push(JournalNote::WaitDispatch(dispatch));
        }
        DispatchResultKind::Success => {
            let outcome = match res.dispatch.kind {
                DispatchKind::Init => DispatchOutcome::InitSuccess {
                    message_id,
                    origin,
                    program_id,
                },
                _ => DispatchOutcome::Success(message_id),
            };

            journal.push(JournalNote::MessageDispatched(outcome));
            journal.push(JournalNote::MessageConsumed(message_id));
            if value != 0 {
                // Send value further
                journal.push(JournalNote::SendValue {
                    from: origin,
                    to: Some(program_id),
                    value
                });
            }
        }
        // Handled in other function
        _ => {
            unreachable!()
        }
    };

    journal
}
