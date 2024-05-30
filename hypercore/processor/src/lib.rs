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
use primitive_types::H256;
use std::{collections::HashMap, ptr};
use wasmtime::{
    AsContext, Caller, Engine, Extern, ImportType, Instance, Linker, Memory, MemoryType, Module,
    Store,
};

pub struct Processor {
    db: Box<dyn hypercore_db::Database>,
}

impl Processor {
    pub fn new(db: Box<dyn hypercore_db::Database>) -> Self {
        Self { db }
    }

    fn execute(&mut self) -> anyhow::Result<()> {
        let mut store: Store<()> = Store::default();

        let module = Module::new(store.engine(), hypercore_runtime::WASM_BINARY)?;

        let mut linker = Linker::new(store.engine());

        linker.func_wrap(
            "env",
            "debug",
            move |mut caller: Caller<'_, ()>, ptr: u32, len: u32| {
                let mut buffer = vec![0; len as usize];

                let mem = caller.get_export("memory").unwrap().into_memory().unwrap();
                mem.read(caller, ptr as usize, &mut buffer).unwrap();

                let message = unsafe { std::str::from_utf8_unchecked(&buffer) };

                log::debug!("Program said: {message:?}");
            },
        )?;

        let instance = linker.instantiate(&mut store, &module)?;

        let greet = instance.get_typed_func::<(), ()>(&mut store, "greet")?;

        greet.call(&mut store, ())?;

        Ok(())
    }

    // TODO: use proper `Dispatch` type here instead of db's.
    pub fn run(
        &mut self,
        chain_head: H256,
        programs: Vec<ProgramId>,
        messages: HashMap<ProgramId, Vec<Message>>,
    ) -> Result<()> {
        self.execute()?;

        Ok(())
    }
}
