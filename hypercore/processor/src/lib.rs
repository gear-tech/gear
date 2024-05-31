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
use parity_wasm::elements::{Internal as PwasmInternal, Module as PwasmModule};
use primitive_types::H256;
use std::{collections::HashMap, ptr};
use wasmtime::{
    AsContext, Caller, Engine, Extern, ImportType, Instance, Linker, Memory, MemoryType, Module,
    Store,
};

mod host;

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
        use rand::Rng as _;
        let len = rand::random::<u8>();
        let mut v = vec![0u8; len as usize];
        rand::thread_rng().fill(&mut v[..]);

        let code = hypercore_db::Code(v);
        let code_hash = code.hash();
        let program_id = code_hash.to_fixed_bytes().into();

        self.db.write_code(&code);

        let mut executor = host::Executor::new(program_id, self.db.clone_boxed())?;

        executor.greet()?;
        executor.read_code()?;

        Ok(())
    }
}
