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
    common::{Dispatch, DispatchResultKind, JournalNote, ProcessResult, ResourceLimiter},
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

    journal.push(JournalNote::GasBurned {
        origin,
        amount: dispatch_result.gas_burned(),
    });

    for (page_number, data) in dispatch_result.page_update() {
        journal.push(JournalNote::UpdatePage {
            origin,
            program_id: dispatch_result.program_id(),
            page_number,
            data,
        })
    }

    for message in dispatch_result.outgoing() {
        journal.push(JournalNote::SendMessage { origin, message });
    }

    for message_id in dispatch_result.awakening() {
        journal.push(JournalNote::WakeMessage { origin, message_id });
    }

    match dispatch_result.kind() {
        DispatchResultKind::Success => journal.push(JournalNote::MessageConsumed(origin)),
        DispatchResultKind::Trap(_) => {
            if let Some(message) = dispatch_result.trap_reply() {
                journal.push(JournalNote::SendMessage { origin, message })
            }

            journal.push(JournalNote::MessageConsumed(origin))
        }
        DispatchResultKind::Wait => {
            journal.push(JournalNote::WaitDispatch(dispatch_result.dispatch()))
        }
    }

    let program = dispatch_result.program();

    ProcessResult { program, journal }
}

pub fn process_many<E: Environment<Ext>>(
    mut programs: BTreeMap<ProgramId, Program>,
    dispatches: Vec<Dispatch>,
    resource_limiter: &mut dyn ResourceLimiter,
    block_info: BlockInfo,
) -> Vec<JournalNote> {
    let mut dispatches = dispatches.into_iter();
    let mut not_processed = Vec::new();
    let mut journal = Vec::new();

    for dispatch in dispatches.by_ref() {
        if !resource_limiter.can_process(&dispatch) {
            not_processed.push(dispatch);
            break;
        }

        resource_limiter.pay_for(&dispatch);

        let program = programs
            .remove(&dispatch.message.dest())
            .expect("Program wasn't found in programs");

        let ProcessResult {
            mut program,
            journal: current_journal,
        } = process::<E>(program, dispatch, block_info);

        for note in &current_journal {
            if let JournalNote::UpdatePage {
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

    not_processed.extend(dispatches);
    journal.push(JournalNote::NotProcessed(not_processed));

    journal
}
