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

use alloc::{collections::BTreeMap, vec, vec::Vec};

use crate::{
    common::{Dispatch, DispatchKind, DispatchResultKind, JournalNote, ProcessResult},
    configs::{BlockInfo, ExecutionSettings},
    executor,
    ext::Ext,
};

use gear_backend_common::Environment;

use gear_core::program::{Program, ProgramId};

pub fn process<E: Environment<Ext>>(
    program: Program,
    dispatch: Dispatch,
    block_info: BlockInfo,
) -> ProcessResult {
    let mut journal = Vec::new();
    let execution_settings = ExecutionSettings::new(block_info);
    let origin = dispatch.message.id();
    let program_id = program.id();

    let mut dispatch_result =
        match executor::execute_wasm::<E>(program, dispatch, execution_settings) {
            Ok(res) => res,
            Err(e) => {
                return ProcessResult {
                    journal: vec![
                        JournalNote::GasBurned {
                            origin,
                            amount: e.gas_burned,
                        },
                        JournalNote::ExecutionFail {
                            origin,
                            program_id: e.program.id(),
                            reason: e.reason,
                        },
                    ],
                    program: e.program,
                }
            }
        };

    dispatch_result.apply_nonce();

    for (page_number, data) in dispatch_result.page_update() {
        journal.push(JournalNote::UpdatePage {
            origin,
            program_id,
            page_number,
            data,
        })
    }

    for message in dispatch_result.outgoing() {
        journal.push(JournalNote::SendMessage { origin, message });
    }

    for message_id in dispatch_result.awakening() {
        journal.push(JournalNote::WakeMessage {
            origin,
            program_id,
            message_id,
        });
    }

    match dispatch_result.kind() {
        DispatchResultKind::Success => {
            journal.push(JournalNote::MessageConsumed(origin));

            if let DispatchKind::Init = dispatch_result.dispatch().kind {
                journal.push(JournalNote::SubmitProgram {
                    owner: dispatch_result.message_source(),
                    program: dispatch_result.program(),
                })
            }
        }
        DispatchResultKind::Trap(trap) => {
            if let Some(message) = dispatch_result.trap_reply() {
                journal.push(JournalNote::SendMessage { origin, message })
            }

            journal.push(JournalNote::MessageTrap { origin, trap });

            journal.push(JournalNote::MessageConsumed(origin))
        }
        DispatchResultKind::Wait => {
            journal.push(JournalNote::WaitDispatch(dispatch_result.dispatch()));
        }
    }

    journal.push(JournalNote::UpdateNonce {
        origin,
        program_id: dispatch_result.program_id(),
        nonce: dispatch_result.message_nonce(),
    });

    journal.push(JournalNote::GasBurned {
        origin,
        amount: dispatch_result.gas_burned(),
    });

    ProcessResult {
        program: dispatch_result.program(),
        journal,
    }
}

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
            if let JournalNote::UpdateNonce {
                origin: _origin,
                program_id: _program_id,
                nonce,
            } = note
            {
                program.set_message_nonce(*nonce);
            } else if let JournalNote::UpdatePage {
                origin: _origin,
                program_id: _program_id,
                page_number,
                data,
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
