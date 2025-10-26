// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::wasm::{
    interface::database_ri,
    storage::{NativeRuntimeInterface, RuntimeInterfaceStorage},
};
use core_processor::configs::BlockInfo;
use ethexe_runtime_common::{
    ProcessingQueueKind, ProgramJournals, RuntimeInterface, process_queue, state::Storage,
};
use gear_core::code::{CodeMetadata, InstrumentedCode};
use gprimitives::{ActorId, H256};

pub fn run(
    program_id: ActorId,
    state_root: H256,
    queue_kind: ProcessingQueueKind,
    maybe_instrumented_code: Option<InstrumentedCode>,
    code_metadata: Option<CodeMetadata>,
    gas_allowance: u64,
) -> (ProgramJournals, u64) {
    log::debug!("You're calling 'run(..)'");

    let block_info = BlockInfo {
        height: database_ri::get_block_height(),
        timestamp: database_ri::get_block_timestamp(),
    };

    let ri = NativeRuntimeInterface {
        block_info,
        storage: RuntimeInterfaceStorage,
    };

    let program_state = ri.storage().program_state(state_root).unwrap();

    let (journals, gas_spent) = process_queue(
        program_id,
        program_state,
        queue_kind,
        maybe_instrumented_code,
        code_metadata,
        &ri,
        gas_allowance,
    );

    for (journal, origin, call_reply) in &journals {
        for note in journal {
            log::debug!("{note:?}");
        }
        log::debug!("Origin: {origin:?}, call_reply {call_reply:?}");
    }

    (journals, gas_spent)
}
