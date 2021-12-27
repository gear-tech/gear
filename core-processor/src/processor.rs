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
        Dispatch, DispatchKind, DispatchOutcome, DispatchResultKind, JournalNote, ProcessResult,
    },
    configs::{BlockInfo, ExecutionSettings},
    executor,
    ext::Ext,
};
use alloc::{collections::BTreeMap, vec::Vec};
use gear_backend_common::Environment;
use gear_core::program::{Program, ProgramId};

/// Process program & dispatch for it and return journal for updates.
pub fn process<E: Environment<Ext>>(
    program: Program,
    dispatch: Dispatch,
    block_info: BlockInfo,
) -> ProcessResult {
    let mut journal = Vec::new();
    let execution_settings = ExecutionSettings::new(block_info);

    let message_id = dispatch.message.id();
    let origin = dispatch.message.source();
    let program_id = program.id();

    let kind = dispatch.kind;

    let mut dispatch_result =
        match executor::execute_wasm::<E>(program, dispatch, execution_settings) {
            Ok(res) => res,
            Err(e) => {
                if let DispatchKind::Init = kind {
                    journal.push(JournalNote::MessageDispatched(
                        DispatchOutcome::InitFailure {
                            message_id,
                            origin,
                            program_id,
                            reason: e.reason,
                        },
                    ));
                } else {
                    // TODO: generate trap reply here
                    journal.push(JournalNote::MessageDispatched(
                        DispatchOutcome::MessageTrap {
                            message_id,
                            trap: Some(e.reason),
                        },
                    ))
                };

                journal.push(JournalNote::GasBurned {
                    message_id,
                    origin,
                    amount: e.gas_counter_view.burned(),
                });
                journal.push(JournalNote::MessageConsumed(message_id));

                return ProcessResult {
                    journal,
                    program: e.program,
                };
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
            if let DispatchKind::Init = kind {
                journal.push(JournalNote::MessageDispatched(
                    DispatchOutcome::InitSuccess {
                        message_id,
                        origin,
                        program: dispatch_result.program.clone(),
                    },
                ))
            } else {
                journal.push(JournalNote::MessageDispatched(DispatchOutcome::Success(
                    message_id,
                )));
            };

            journal.push(JournalNote::GasBurned {
                message_id,
                origin,
                amount: dispatch_result.gas_counter_view.burned(),
            });
            journal.push(JournalNote::MessageConsumed(message_id));
        }
        DispatchResultKind::Trap(trap) => {
            if let Some(message) =
                dispatch_result.trap_reply(dispatch_result.gas_counter_view.left())
            {
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
                ))
            } else {
                journal.push(JournalNote::MessageDispatched(
                    DispatchOutcome::MessageTrap { message_id, trap },
                ));
            }

            journal.push(JournalNote::GasBurned {
                message_id,
                origin,
                amount: dispatch_result.gas_counter_view.burned(),
            });

            journal.push(JournalNote::MessageConsumed(message_id));
        }
        DispatchResultKind::Wait => {
            journal.push(JournalNote::GasBurned {
                message_id,
                origin,
                amount: dispatch_result.gas_counter_view.burned(),
            });

            dispatch_result.dispatch.message.gas_limit = dispatch_result.gas_counter_view.left();

            journal.push(JournalNote::WaitDispatch(dispatch_result.dispatch));
        }
    }

    journal.push(JournalNote::UpdateNonceAndPagesAmount {
        program_id,
        persistent_pages: dispatch_result.persistent_pages,
        nonce: dispatch_result.nonce,
    });

    for (page_number, data) in dispatch_result.page_update {
        journal.push(JournalNote::UpdatePage {
            program_id,
            page_number,
            data,
        })
    }

    ProcessResult {
        program: dispatch_result.program,
        journal,
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
            .remove(&dispatch.message.dest())
            .expect("Program wasn't found in programs");

        let ProcessResult {
            mut program,
            journal: current_journal,
        } = process::<E>(program, dispatch, block_info);

        for note in &current_journal {
            if let JournalNote::UpdateNonceAndPagesAmount { nonce, .. } = note {
                program.set_message_nonce(*nonce);
            } else if let JournalNote::UpdatePage {
                page_number, data, ..
            } = note
            {
                program.set_page(*page_number, data).expect("Can't fail");
            }
        }

        programs.insert(program.id(), program);

        journal.extend(current_journal);
    }

    journal
}
