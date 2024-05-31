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

use anyhow::Result;
use gear_core::ids::ProgramId;
use log::Level;
use runtime::Runtime;
use wasmtime::{AsContextMut, Caller, Engine, Instance, Linker, Module, Store};

mod calls;
mod runtime;

pub(crate) mod utils;

pub struct HostState {
    program_id: ProgramId,
    db: Box<dyn hypercore_db::Database>,
}

pub struct Executor {
    store: Store<HostState>,
    instance: Instance,
}

impl Executor {
    pub fn new(program_id: ProgramId, db: Box<dyn hypercore_db::Database>) -> Result<Self> {
        let mut runtime = Runtime::new();

        runtime.add_start_section();

        let mut store: Store<HostState> =
            Store::new(&Engine::default(), HostState { program_id, db });
        let module = Module::new(store.engine(), runtime.into_bytes())?;

        let mut linker = Linker::new(store.engine());

        // Logging host module.
        linker.func_wrap("env", "logging_log_v1", calls::logging::log)?;
        linker.func_wrap("env", "logging_max_level_v1", calls::logging::max_level)?;

        // Code host module.
        linker.func_wrap("env", "code_len_v1", calls::code::len)?;
        linker.func_wrap("env", "code_read_v1", calls::code::read)?;

        // Tmp host module.
        linker.func_wrap("env", "program_id", calls::program_id)?;

        let instance = linker.instantiate(&mut store, &module)?;

        Ok(Self { store, instance })
    }

    pub fn greet(&mut self) -> Result<()> {
        let func = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "greet")?;

        func.call(&mut self.store, ())?;

        Ok(())
    }

    pub fn read_code(&mut self) -> Result<()> {
        let func = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "read_code")?;

        func.call(&mut self.store, ())?;

        Ok(())
    }
}
