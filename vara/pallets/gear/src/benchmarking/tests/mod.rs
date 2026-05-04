// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! This module contains pallet tests usually defined under "std" feature in the separate `tests` module.
//! The reason of moving them here is an ability to run these tests with different execution environments
//! (native or wasm, i.e. using wasmi or sandbox executors). When "std" is enabled we can run them on wasmi,
//! when it's not (only "runtime-benchmarks") - sandbox will be turned on.

use super::*;

pub mod lazy_pages;
pub mod syscalls_integrity;
mod utils;

use crate::{
    Ext, HandleKind,
    benchmarking::{
        code::{WasmModule, body},
        utils as common_utils,
    },
};
use common::benchmarking;
use gear_wasm_instrument::Instruction;

pub fn check_stack_overflow<T>()
where
    T: Config,
    T::AccountId: Origin,
{
    let instrs = vec![Instruction::Call(0)];

    let module: WasmModule<T> = ModuleDefinition {
        memory: Some(ImportedMemory::new(0)),
        init_body: Some(body::from_instructions(instrs)),
        ..Default::default()
    }
    .into();

    let source = benchmarking::account("instantiator", 0, 0);
    let exec = common_utils::prepare_exec::<T>(
        source,
        HandleKind::Init(module.code),
        Default::default(),
        Default::default(),
    )
    .unwrap();

    let dispatch =
        core_processor::process::<Ext>(&exec.block_config, exec.context, exec.random_data)
            .unwrap()
            .into_iter()
            .find_map(|note| match note {
                JournalNote::SendDispatch { dispatch, .. } => Some(dispatch),
                _ => None,
            })
            .unwrap();

    let code = dispatch
        .reply_details()
        .expect("reply details")
        .to_reply_code();
    assert_eq!(
        code,
        ReplyCode::Error(ErrorReplyReason::Execution(
            SimpleExecutionError::StackLimitExceeded
        ))
    );
}
