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
        Dispatch, DispatchKind, DispatchOutcome, DispatchResultKind, JournalNote,
        SendValueKind, SendValueKindFactory,
    },
    configs::{BlockInfo, ExecutionSettings},
    executor,
    ext::Ext,
};
use alloc::{collections::BTreeMap, vec::Vec};
use gear_backend_common::Environment;
use gear_core::{
    message::{self, Message},
    program::{Program, ProgramId},
};

/// Process program & dispatch for it and return journal for updates.
pub fn process<E: Environment<Ext>>(
    program: Option<Program>,
    dispatch: Dispatch,
    block_info: BlockInfo,
) -> Vec<JournalNote> {
    let mut journal = Vec::new();

    let message_id = dispatch.message.id();
    let origin = dispatch.message.source();

    let send_value_factory = SendValueKindFactory::new(dispatch.message.value());

    if program.is_none() {
        assert!(matches!(dispatch.kind, DispatchKind::None));

        let send_value = send_value_factory.send_back(origin);

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

        // Reply message destination could not have `handle_reply` method.
        // TODO [discuss before merge]: If it's not intended to instantiate `Program` with `handle_reply` method existence check,
        // then such system reply message should not be sent for programs without `handle_reply`, because execution of such messages
        // will take gas for loading memory. It seems to be stealing gas from users for a reply message, that the user didn't want to
        // handle in the program.
        journal.push(JournalNote::SendMessage {
            message_id,
            message: reply_message,
        });
        journal.push(JournalNote::MessageDispatched(
            CoreDispatchOutcome::Skip(message_id),
            send_value,
        ));
        journal.push(JournalNote::MessageConsumed(message_id));

        todo!("todo [sab] return")
    }

    let program = program.expect("was checked before");
    let program_id = program.id();
    assert_eq!(program_id, dispatch.message.dest());

    let kind = dispatch.kind;

    let mut dispatch_result =
        match executor::execute_wasm::<E>(program, dispatch, execution_settings) {
            Ok(res) => res,
            Err(e) => {
                let send_value = send_value_factory.send_back(origin);
                if let DispatchKind::Init = kind {
                    journal.push(JournalNote::MessageDispatched(
                        DispatchOutcome::InitFailure {
                            message_id,
                            origin,
                            program_id,
                            reason: e.reason,
                        },
                        send_value,
                    ));
                } else {
                    // TODO: generate trap reply here
                    journal.push(JournalNote::MessageDispatched(
                        DispatchOutcome::MessageTrap {
                            message_id,
                            program_id,
                            trap: Some(e.reason),
                        },
                        send_value,
                    ))
                };

                journal.push(JournalNote::GasBurned {
                    message_id,
                    origin,
                    amount: e.gas_amount.burned(),
                });
                journal.push(JournalNote::MessageConsumed(message_id));

                return journal;
            }
        };

    for message in dispatch_result.outgoing.clone() {
        journal.push(JournalNote::SendMessage {
            message_id,
            message,
        });
    }

    for awakening_id in dispatch_result.awakening.clone() {
        journal.push(JournalNote::WakeMessage {
            message_id,
            program_id,
            awakening_id,
        });
    }

    match dispatch_result.kind {
        DispatchResultKind::Success => {
            let send_value = send_value_factory.send_further(origin, program_id);
            if let DispatchKind::Init = kind {
                journal.push(JournalNote::MessageDispatched(
                    DispatchOutcome::InitSuccess {
                        message_id,
                        origin,
                        program_id,
                    },
                    send_value,
                ))
            } else {
                journal.push(JournalNote::MessageDispatched(
                    DispatchOutcome::Success(message_id),
                    send_value,
                ));
            };

            journal.push(JournalNote::GasBurned {
                message_id,
                origin,
                amount: dispatch_result.gas_amount.burned(),
            });
            journal.push(JournalNote::MessageConsumed(message_id));
        }
        DispatchResultKind::Trap(trap) => {
            let send_value = send_value_factory.send_back(origin);
            if let Some(message) = dispatch_result.trap_reply(dispatch_result.gas_amount.left()) {
                journal.push(JournalNote::SendMessage {
                    message_id,
                    message,
                })
            }

            if let DispatchKind::Init = kind {
                journal.push(JournalNote::MessageDispatched(
                    DispatchOutcome::InitFailure {
                        message_id,
                        origin,
                        program_id,
                        reason: trap.unwrap_or_default(),
                    },
                    send_value,
                ))
            } else {
                journal.push(JournalNote::MessageDispatched(
                    DispatchOutcome::MessageTrap {
                        message_id,
                        program_id,
                        trap,
                    },
                    send_value,
                ));
            }

            journal.push(JournalNote::GasBurned {
                message_id,
                origin,
                amount: dispatch_result.gas_amount.burned(),
            });

            journal.push(JournalNote::MessageConsumed(message_id));
        }
        DispatchResultKind::Wait => {
            journal.push(JournalNote::GasBurned {
                message_id,
                origin,
                amount: dispatch_result.gas_amount.burned(),
            });

            dispatch_result.dispatch.message.gas_limit = dispatch_result.gas_amount.left();

            journal.push(JournalNote::WaitDispatch(dispatch_result.dispatch));
        }
    }

    journal.push(JournalNote::UpdateNonce {
        program_id,
        nonce: dispatch_result.nonce,
    });

    for (page_number, data) in dispatch_result.page_update {
        journal.push(JournalNote::UpdatePage {
            program_id,
            page_number,
            data,
        })
    }

    journal
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

        let current_journal = process::<E>(program.clone(), dispatch, block_info);

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
