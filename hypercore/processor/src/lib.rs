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
use gear_core::ids::{prelude::CodeIdExt, CodeId, ProgramId};
use hypercore_db::{Code, Message};
use log::Level;
use parity_wasm::elements::{Internal as PwasmInternal, Module as PwasmModule};
use primitive_types::H256;
use std::{collections::HashMap, ptr};
use wasmtime::{
    AsContext, Caller, Engine, Extern, ImportType, Instance, Linker, Memory, MemoryType, Module,
    Store,
};
use hypercore_observer::EventsBlock;

mod host;

pub struct Processor {
    db: Box<dyn hypercore_db::Database>,
}

impl Processor {
    pub fn new(db: Box<dyn hypercore_db::Database>) -> Self {
        Self { db }
    }

    pub fn new_code(&mut self, hash: H256, code: Vec<u8>) -> Result<bool> {
        if CodeId::from(hash) != CodeId::generate(&code) {
            return Ok(false);
        }

        let mut executor = host::Executor::verifier(code)?;

        let res = executor.verify()?;

        if res {
            let store = executor.into_store();
            self.db.write_code(&Code(store.into_data().code))
        }

        Ok(res)
    }

    // TODO: use proper `Dispatch` type here instead of db's.
    pub fn run(
        &mut self,
        chain_head: H256,
        programs: Vec<ProgramId>,
        messages: HashMap<ProgramId, Vec<Message>>,
    ) -> Result<()> {
        Ok(())
    }

    pub fn process_block_events(&mut self, events: EventsBlock) -> Result<()> {
        log::debug!("Processing events for {:?}", events.block_hash);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hypercore_db::{Database, MemDb};
    use wabt::wat2wasm;

    fn valid_code() -> Code {
        let wat = r#"
            (module
            (import "env" "memory" (memory 1))
            (import "env" "gr_reply" (func $reply (param i32 i32 i32 i32)))
            (export "init" (func $init))
            (func $init
                (call $reply (i32.const 0) (i32.const 32) (i32.const 222) (i32.const 333))
            )
        )"#;

        Code(wat2wasm(wat).unwrap())
    }

    fn init_logger() {
        let _ = env_logger::Builder::from_default_env()
            .format_module_path(false)
            .format_level(true)
            .try_init();
    }

    #[test]
    fn verify_code() {
        init_logger();

        let db = MemDb::new();
        let mut processor = Processor::new(db.clone_boxed());

        let valid = valid_code();
        let valid_hash = valid.hash();

        assert!(db.read_code(valid_hash).is_none());
        assert!(processor.new_code(valid_hash, valid.0).unwrap());
        assert!(db.read_code(valid_hash).is_some());

        let invalid = Code(vec![0; 42]);
        let invalid_hash = invalid.hash();

        assert!(db.read_code(invalid_hash).is_none());
        assert!(!processor.new_code(invalid_hash, invalid.0).unwrap());
        assert!(db.read_code(invalid_hash).is_none());
    }

    #[test]
    fn bad_hash() {
        init_logger();

        let db = MemDb::new();
        let mut processor = Processor::new(db.clone_boxed());

        let valid = valid_code();
        let valid_hash = valid.hash();

        assert!(processor.new_code(valid_hash, valid.0.clone()).unwrap());
        assert!(!processor.new_code(H256::zero(), valid.0).unwrap());
    }
}
