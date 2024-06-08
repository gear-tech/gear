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

use alloc::vec::Vec;
use gear_lazy_pages_interface::{LazyPagesInterface as _, LazyPagesRuntimeInterface};
use gear_sandbox::{
    host_executor::{Caller, EnvironmentDefinitionBuilder, Instance, Memory, Store},
    HostError, ReturnValue, SandboxEnvironmentBuilder, SandboxInstance, SandboxMemory,
    SandboxStore, Value,
};
use gear_sandbox_env::WasmReturnValue;

fn system_break(
    _caller: &mut Caller<'_, ()>,
    _args: &[Value],
) -> Result<WasmReturnValue, HostError> {
    Ok(WasmReturnValue {
        gas: 0,
        inner: ReturnValue::Unit,
    })
}

fn reply_mock(_caller: &mut Caller<'_, ()>, _args: &[Value]) -> Result<WasmReturnValue, HostError> {
    log::debug!("Reply was called, congratulations!");
    Ok(WasmReturnValue {
        gas: 0,
        inner: ReturnValue::Unit,
    })
}

pub fn run(code: Vec<u8>) {
    log::info!("You're calling 'run(..)'");

    let mut store = Store::new(());
    let memory = Memory::new(&mut store, 1, None).unwrap();

    let mut env_def_builder = EnvironmentDefinitionBuilder::new();

    env_def_builder.add_memory("env", "memory", memory);

    env_def_builder.add_host_func("env", "gr_system_break", system_break);
    env_def_builder.add_host_func("env", "gr_reply", reply_mock);

    let mut instance = Instance::new(&mut store, code.as_ref(), &env_def_builder).unwrap();

    let arr: &[u8] = &[];
    let mut gas = 0;

    // TODO: init lazy pages for program.
    // assert!(LazyPagesRuntimeInterface::pre_process_memory_accesses(
    //     arr.as_ref(),
    //     arr.as_ref(),
    //     &mut gas
    // )
    // .is_err());

    let res = instance.invoke(&mut store, "init", &[]).unwrap();
    log::debug!("Sandbox execution result = {res:?}");
    let _res = res;
}
