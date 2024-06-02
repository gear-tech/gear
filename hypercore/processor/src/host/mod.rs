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
use context::VerifierContext;
use gear_core::{
    code::InstrumentedCode,
    ids::{CodeId, ProgramId},
};
use log::Level;
use parity_scale_codec::Decode;
use runtime::Runtime;
use wasmtime::{AsContextMut, Caller, Engine, Instance, Linker, Memory, Module, Store};

mod calls;
mod runtime;

pub(crate) mod context;
pub(crate) mod utils;

pub struct Executor<T> {
    store: Store<T>,
    instance: Instance,
}

impl<T: 'static> Executor<T> {
    fn new(state: T, linking_fn: impl Fn(&mut Linker<T>) -> Result<()>) -> Result<Self> {
        let mut runtime = Runtime::new();
        runtime.add_start_section();

        let mut store = Store::new(&Engine::default(), state);

        let module = Module::new(store.engine(), runtime.into_bytes())?;

        let mut linker = Linker::new(store.engine());

        // Logging host module.
        calls::logging::link(&mut linker)?;

        linking_fn(&mut linker)?;

        let instance = linker.instantiate(&mut store, &module)?;

        Ok(Self { store, instance })
    }

    pub fn memory(&mut self) -> Memory {
        self.instance
            .get_export(&mut self.store, "memory")
            .unwrap()
            .into_memory()
            .unwrap()
    }

    pub fn into_store(self) -> Store<T> {
        self.store
    }
}

impl Executor<VerifierContext> {
    pub fn verifier(code: Vec<u8>) -> Result<Self> {
        let state = VerifierContext { code };

        let linking_fn = |linker: &mut Linker<VerifierContext>| {
            // Code host module.
            calls::code::link(linker)?;

            Ok(())
        };

        Self::new(state, linking_fn)
    }

    pub fn verify(&mut self) -> Result<bool> {
        let func = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "verify")?;

        let len = self.store.data().code.len() as i32;

        let res = func.call(&mut self.store, len)?;

        Ok(res == 0)
    }

    pub fn instrument(&mut self) -> Result<Option<InstrumentedCode>> {
        let func = self
            .instance
            .get_typed_func::<i32, i64>(&mut self.store, "instrument")?;

        let len = self.store.data().code.len() as i32;

        let res = func.call(&mut self.store, len)?;

        if res == 0 {
            return Ok(None);
        }

        let [ptr, len]: [i32; 2] = unsafe { core::mem::transmute(res) };

        let mut buffer = vec![0; len as usize];

        let memory = self.memory();

        memory
            .read(&mut self.store, ptr as usize, &mut buffer)
            .unwrap();

        let instrumented = Decode::decode(&mut buffer.as_ref())?;

        Ok(Some(instrumented))
    }
}
