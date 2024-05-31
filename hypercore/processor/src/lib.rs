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

//! Program's execution service for eGPU.

use anyhow::Result;
use gear_core::ids::ProgramId;
use hypercore_db::Message;
use log::Level;
use primitive_types::H256;
use std::{collections::HashMap, ptr};
use wasmtime::{
    AsContext, Caller, Engine, Extern, ImportType, Instance, Linker, Memory, MemoryType, Module,
    Store,
};

pub struct HostState {
    program_id: ProgramId,
}

pub struct Processor {
    db: Box<dyn hypercore_db::Database>,
}

impl Processor {
    pub fn new(db: Box<dyn hypercore_db::Database>) -> Self {
        Self { db }
    }

    // TODO: use proper `Dispatch` type here instead of db's.
    pub fn run(
        &mut self,
        chain_head: H256,
        programs: Vec<ProgramId>,
        messages: HashMap<ProgramId, Vec<Message>>,
    ) -> Result<()> {
        let program_id = messages.keys().next().cloned().unwrap_or_default();

        let mut store: Store<HostState> = Store::new(&Engine::default(), HostState { program_id });

        let module = Module::new(store.engine(), hypercore_runtime::WASM_BINARY)?;

        let mut linker = Linker::new(store.engine());

        linker.func_wrap(
            "env",
            "logging_log_v1",
            |mut caller: Caller<'_, HostState>, level: i32, target: i64, message: i64| {
                let level = match level {
                    1 => Level::Error,
                    2 => Level::Warn,
                    3 => Level::Info,
                    4 => Level::Debug,
                    _ => Level::Trace,
                };

                let mem = caller.get_export("memory").unwrap().into_memory().unwrap();
                let target = utils::read_ri_slice(&mem, &mut caller, target);
                let target = core::str::from_utf8(&target).unwrap_or_default();

                let message = utils::read_ri_slice(&mem, &mut caller, message);
                let message = core::str::from_utf8(&message).unwrap_or_default();

                log::log!(target: target, level, "{message}");
            },
        )?;

        linker.func_wrap(
            "env",
            "logging_max_level_v1",
            |_: Caller<'_, HostState>| -> i32 { log::max_level() as usize as i32 },
        )?;

        linker.func_wrap(
            "env",
            "program_id",
            |mut caller: Caller<'_, HostState>, ptr: u32| {
                let program_id = caller.data().program_id;

                let mem = caller.get_export("memory").unwrap().into_memory().unwrap();
                mem.write(caller, ptr as usize, program_id.as_ref())
                    .unwrap();
            },
        )?;

        let instance = linker.instantiate(&mut store, &module)?;

        let greet = instance.get_typed_func::<(), ()>(&mut store, "greet")?;

        greet.call(&mut store, ())?;

        Ok(())
    }
}

mod utils {
    use super::*;

    pub fn read_ri_slice(memory: &Memory, store: &mut Caller<'_, HostState>, data: i64) -> Vec<u8> {
        let data_bytes = data.to_le_bytes();

        let mut ptr_bytes = [0; 4];
        ptr_bytes.copy_from_slice(&data_bytes[..4]);
        let ptr = i32::from_le_bytes(ptr_bytes);

        let mut len_bytes = [0; 4];
        len_bytes.copy_from_slice(&data_bytes[4..]);
        let len = i32::from_le_bytes(len_bytes);

        let mut buffer = vec![0; len as usize];

        memory.read(store, ptr as usize, &mut buffer).unwrap();

        buffer
    }
}
