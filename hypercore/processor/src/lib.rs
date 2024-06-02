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

use crate::host::{db::Database, state::Message};
use anyhow::Result;
use gear_core::ids::{prelude::CodeIdExt, CodeId, ProgramId};
use host::context::CodeContext;
use hypercore_db::CASDatabase;
use hypercore_observer::Event;
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
    db: Database,
}

impl Processor {
    pub fn new(db: Box<dyn CASDatabase>) -> Self {
        Self {
            db: Database::new(db),
        }
    }

    pub fn new_code(&mut self, hash: CodeId, code: Vec<u8>) -> Result<bool> {
        if hash != CodeId::generate(&code) {
            return Ok(false);
        }

        let mut executor = host::Executor::verifier(code)?;

        let res = executor.verify()?;

        if res {
            let context = executor.into_store().into_data();
            let code_id = CodeContext::id(&context);
            self.db.write_code(code_id, &context.code)
        }

        Ok(res)
    }

    pub fn instrument_code(&mut self, code_id: CodeId) -> Result<Option<H256>> {
        let code = self.db.read_code(code_id).unwrap();

        let mut executor = host::Executor::verifier(code)?;

        if let Some(instrumented) = executor.instrument()? {
            let hash = self.db.write_instrumented_code(&instrumented);

            Ok(Some(hash))
        } else {
            Ok(None)
        }
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

    pub fn process_observer_event(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::UploadCode { code_id, .. } => {
                log::debug!("Processing upload code {code_id:?}");
            }
            Event::Block {
                ref block_hash,
                events: _,
            } => {
                log::debug!("Processing events for {block_hash:?}");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hypercore_db::MemDb;
    use wabt::wat2wasm;

    fn valid_code() -> Vec<u8> {
        let wat = r#"
            (module
            (import "env" "memory" (memory 1))
            (import "env" "gr_reply" (func $reply (param i32 i32 i32 i32)))
            (export "init" (func $init))
            (func $init
                (call $reply (i32.const 0) (i32.const 32) (i32.const 222) (i32.const 333))
            )
        )"#;

        wat2wasm(wat).unwrap()
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

        let db = MemDb::default();
        let mut processor = Processor::new(db.clone_boxed());

        let valid = valid_code();
        let valid_id = CodeId::generate(&valid);

        assert!(processor.db.read_code(valid_id).is_none());
        assert!(processor.new_code(valid_id, valid).unwrap());
        assert!(processor.db.read_code(valid_id).is_some());

        let invalid = vec![0; 42];
        let invalid_id = CodeId::generate(&invalid);

        assert!(processor.db.read_code(invalid_id).is_none());
        assert!(!processor.new_code(invalid_id, invalid).unwrap());
        assert!(processor.db.read_code(invalid_id).is_none());
    }

    #[test]
    fn bad_hash() {
        init_logger();

        let db = MemDb::default();
        let mut processor = Processor::new(db.clone_boxed());

        let valid = valid_code();
        let valid_id = CodeId::generate(&valid).into_bytes().into();

        assert!(processor.new_code(valid_id, valid.clone()).unwrap());
        assert!(!processor.new_code(H256::zero().into(), valid).unwrap());
    }

    #[test]
    fn instrument_code() {
        init_logger();

        let db = MemDb::default();
        let mut processor = Processor::new(db.clone_boxed());

        let code = valid_code();
        let code_len = code.len();
        let id = CodeId::generate(&code);

        assert!(processor.new_code(id, code).unwrap());

        let hash = processor.instrument_code(id).unwrap().unwrap();
        let instrumented = processor.db.read_instrumented_code(hash).unwrap();

        assert_eq!(instrumented.original_code_len() as usize, code_len);
        assert!(instrumented.code().len() > code_len);
    }
}
