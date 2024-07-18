// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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
use alloc::vec::Vec;
use core_processor::{common::JournalNote, configs::BlockInfo};
use ethexe_runtime_common::{process_next_message, state::Storage, RuntimeInterface};
use gear_core::{code::InstrumentedCode, ids::ProgramId};
use gprimitives::{CodeId, H256};

pub fn wake_messages(program_id: ProgramId, state_hash: H256) -> H256 {
    log::info!("You're calling 'wake_messages(..)'");

    let block_info = BlockInfo {
        height: database_ri::get_block_height(),
        timestamp: database_ri::get_block_timestamp(),
    };

    let ri = NativeRuntimeInterface {
        block_info,
        storage: RuntimeInterfaceStorage,
    };

    let program_state = ri.storage().read_state(state_hash).unwrap();

    ethexe_runtime_common::wake_messages(program_id, program_state, &ri).unwrap_or(state_hash)
}

pub fn run(
    program_id: ProgramId,
    original_code_id: CodeId,
    state_root: H256,
    maybe_instrumented_code: Option<InstrumentedCode>,
) -> Vec<JournalNote> {
    log::info!("You're calling 'run(..)'");

    let block_info = BlockInfo {
        height: database_ri::get_block_height(),
        timestamp: database_ri::get_block_timestamp(),
    };

    let ri = NativeRuntimeInterface {
        block_info,
        storage: RuntimeInterfaceStorage,
    };

    let program_state = ri.storage().read_state(state_root).unwrap();

    let journal = process_next_message(
        program_id,
        program_state,
        maybe_instrumented_code,
        original_code_id,
        &ri,
    );

    log::debug!("Done creating journal: {} notes", journal.len());

    for note in &journal {
        log::debug!("{note:?}");
    }

    journal
}
