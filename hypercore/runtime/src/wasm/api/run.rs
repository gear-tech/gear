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

use crate::wasm::storage::{NativeRuntimeInterface, RuntimeInterfaceStorage};
use alloc::vec::Vec;
use core_processor::{
    common::{ExecutableActorData, JournalNote},
    configs::BlockConfig,
};
use gear_core::{
    code::InstrumentedCode,
    ids::ProgramId,
    message::{DispatchKind, StoredDispatch, StoredMessage},
    pages::{numerated::tree::IntervalsTree, GearPage, WasmPage, WasmPagesAmount},
};
use gear_core_backend::env::Environment;
use gear_lazy_pages_interface::{LazyPagesInterface, LazyPagesRuntimeInterface};
use gear_sandbox::{
    default_executor::{Caller, EnvironmentDefinitionBuilder, Instance, Memory, Store},
    HostError, ReturnValue, SandboxEnvironmentBuilder, SandboxInstance, SandboxMemory,
    SandboxStore, Value,
};
use gear_sandbox_env::WasmReturnValue;
use gprimitives::{CodeId, H256};
use gsys::{GasMultiplier, Percent};
use hypercore_runtime_common::{
    process_next_message, state::Storage, HandlerForPrograms, RuntimeInterface,
};
use parity_scale_codec::Encode;

pub fn run(
    program_id: ProgramId,
    original_code_id: CodeId,
    state_root: H256,
    maybe_instrumented_code: Option<InstrumentedCode>,
) -> Vec<JournalNote> {
    log::info!("You're calling 'run(..)'");

    let ri = NativeRuntimeInterface(RuntimeInterfaceStorage);

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
