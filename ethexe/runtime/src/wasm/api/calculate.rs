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
use core_processor::{configs::BlockInfo, Ext};
use ethexe_runtime_common::{
    state::{ActiveProgram, Program, ProgramState, Storage},
    RuntimeInterface,
};
use gear_core::{code::InstrumentedCode, ids::ProgramId};
use gprimitives::H256;

pub fn reply_for_handle(
    program_id: ProgramId,
    state_root: H256,
    instrumented_code: InstrumentedCode,
    payload: Vec<u8>,
) -> Result<Vec<u8>, String> {
    log::debug!("You're calling 'calculate::reply_for_handle(..)'");

    let block_info = BlockInfo {
        height: database_ri::get_block_height(),
        timestamp: database_ri::get_block_timestamp(),
    };

    let ri = NativeRuntimeInterface {
        block_info,
        storage: RuntimeInterfaceStorage,
    };

    let gas_allowance = 1_000_000_000;

    let ProgramState {
        program:
            Program::Active(ActiveProgram {
                allocations_hash,
                memory_infix,
                initialized: true,
                ..
            }),
        ..
    } = ri.storage().read_state(state_root).unwrap()
    else {
        return Err(String::from("Program is not active and/or initialized"));
    };

    let allocations =
        allocations_hash.with_hash_or_default(|hash| ri.storage().read_allocations(hash));

    let program_info = Some((program_id, memory_infix));

    core_processor::informational::execute_for_reply::<
        Ext<<NativeRuntimeInterface as RuntimeInterface<RuntimeInterfaceStorage>>::LazyPages>,
        String,
    >(
        String::from("handle"),
        instrumented_code,
        allocations,
        program_info,
        payload,
        gas_allowance,
        block_info,
    )
}
