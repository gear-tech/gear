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

use crate::host::db::Database;
use anyhow::Result;
use core_processor::{
    common::{ExecutableActorData, JournalNote},
    configs::{BlockConfig, BlockInfo},
    ContextChargedForCode, ContextChargedForInstrumentation, Ext, ProcessExecutionContext,
};
use gear_core::{
    code::InstrumentedCode,
    ids::{prelude::CodeIdExt, CodeId, ProgramId},
    message::{IncomingDispatch, IncomingMessage},
    program,
};
use gear_lazy_pages::LazyPagesVersion;
use gear_lazy_pages_common::LazyPagesInitContext;
use gear_lazy_pages_native_interface::LazyPagesNative;
use gsys::{GasMultiplier, Percent};
use host::{
    context::HostContext,
    state::{MessageQueue, ProgramState},
};
use hypercore_db::CASDatabase;
use hypercore_observer::Event;
use pages_storage::PagesStorage;
use primitive_types::H256;
use std::collections::HashMap;
use wasmtime::{
    AsContext, Caller, Engine, Extern, ImportType, Instance, Linker, Memory, MemoryType, Module,
    Store,
};

mod host;
mod pages_storage;

pub struct Processor {
    db: Database,
}

impl Processor {
    // TODO: temporary method to run one dispatch, should be removed from here.
    pub fn run_one(&self, program_id: ProgramId, program_state: &ProgramState) -> Vec<JournalNote> {
        let mut queue: MessageQueue = program_state
            .queue_hash
            .read(self.db.inner())
            .unwrap_or_default();

        let Some(dispatch) = queue.0.pop() else {
            return vec![];
        };

        let block_config = BlockConfig {
            block_info: BlockInfo {
                height: 0,    // TODO
                timestamp: 0, // TODO
            },
            performance_multiplier: Percent::new(100),
            forbidden_funcs: Default::default(),
            reserve_for: 125_000_000,
            gas_multiplier: GasMultiplier::from_gas_per_value(1), // TODO
            costs: Default::default(),                            // TODO
            existential_deposit: 0,                               // TODO
            mailbox_threshold: 3000,
            max_reservations: 50,
            max_pages: 512.into(),
            outgoing_limit: 1024,
            outgoing_bytes_limit: 64 * 1024 * 1024,
        };

        let payload = dispatch
            .payload_hash
            .read(self.db.inner())
            .unwrap_or_default();
        let incoming_message = IncomingMessage::new(
            dispatch.id,
            dispatch.source,
            payload,
            dispatch.gas_limit,
            dispatch.value,
            dispatch.details,
        );
        let dispatch = IncomingDispatch::new(dispatch.kind, incoming_message, dispatch.context);

        let precharged_dispatch = core_processor::precharge_for_program(
            &block_config,
            1_000_000_000_000, // TODO
            dispatch,
            program_id,
        )
        .expect("TODO: process precharge errors");

        let code: InstrumentedCode = program_state.instrumented_code_hash.read(self.db.inner());
        let allocations = program_state
            .allocations_hash
            .read(self.db.inner())
            .unwrap_or_default();
        let gas_reservation_map = program_state
            .gas_reservation_map_hash
            .read(self.db.inner())
            .unwrap_or_default();
        let actor_data = ExecutableActorData {
            allocations,
            code_id: program_state.original_code_hash.hash.into(),
            code_exports: code.exports().clone(),
            static_pages: code.static_pages(),
            gas_reservation_map,
            memory_infix: program_state.memory_infix,
        };

        let context = core_processor::precharge_for_code_length(
            &block_config,
            precharged_dispatch,
            program_id,
            actor_data,
        )
        .expect("TODO: process precharge errors");

        let context = ContextChargedForCode::from((context, code.code().len() as u32));
        let context = core_processor::precharge_for_memory(
            &block_config,
            ContextChargedForInstrumentation::from(context),
        )
        .expect("TODO: process precharge errors");

        let execution_context =
            ProcessExecutionContext::from((context, code, program_state.balance));

        let memory_map = program_state
            .pages_hash
            .read(self.db.inner())
            .unwrap_or_default();
        let pages_storage = PagesStorage {
            db: self.db.inner(),
            memory_map,
        };
        gear_lazy_pages::init(
            LazyPagesVersion::Version1,
            LazyPagesInitContext::new(Default::default()),
            pages_storage,
        )
        .expect("Failed to init lazy-pages");

        let random_data = (vec![0; 32], 0);
        core_processor::process::<Ext<LazyPagesNative>>(
            &block_config,
            execution_context,
            random_data,
        )
        .unwrap()

        // TODO: handle inner journal notes and return receipts
    }

    pub fn new(db: Box<dyn CASDatabase>) -> Self {
        Self {
            db: Database::new(db),
        }
    }

    pub fn new_code(&mut self, hash: CodeId, code: Vec<u8>) -> Result<bool> {
        if hash != CodeId::generate(&code) {
            return Ok(false);
        }

        let mut executor = host::Executor::new(HostContext::new(code))?;

        let res = executor.verify()?;

        if res {
            let context = executor.into_store().into_data();
            let code_id = context.id();
            self.db.write_code(code_id, &context.code)
        }

        Ok(res)
    }

    pub fn instrument_code(&mut self, code_id: CodeId) -> Result<Option<H256>> {
        let code = self.db.read_code(code_id).unwrap();

        let mut executor = host::Executor::new(HostContext::new(code))?;

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
        _chain_head: H256,
        _programs: Vec<ProgramId>,
        _messages: HashMap<ProgramId, Vec<IncomingMessage>>,
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
    use std::collections::BTreeMap;

    use super::*;
    use gear_core::{
        message::{DispatchKind, Payload},
        pages::GearPage,
    };
    use host::state::{Dispatch, HashAndLen, MaybeHash, MessageQueue};
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

    #[test]
    fn run_one() {
        init_logger();

        let db = MemDb::default();
        let mut processor = Processor::new(db.clone_boxed());

        let code = demo_ping::WASM_BINARY;
        let code_id = CodeId::generate(code);
        assert!(processor.new_code(code_id, code.to_vec()).unwrap());

        let instrumented_code_hash = processor.instrument_code(code_id).unwrap().unwrap();

        let payload = Payload::try_from(b"PING".to_vec()).unwrap();
        let payload_hash = processor.db.write(&payload);

        let dispatch = Dispatch {
            id: Default::default(),
            kind: DispatchKind::Handle,
            source: Default::default(),
            payload_hash: payload_hash.into(),
            gas_limit: 1_000_000_000,
            value: 1,
            details: None,
            context: None,
        };

        let queue = MessageQueue(vec![dispatch]);
        let queue_hash = processor.db.write(&queue);

        let program_id = ProgramId::default();
        let program_state = ProgramState {
            queue_hash: queue_hash.into(),
            allocations_hash: MaybeHash::Empty,
            pages_hash: MaybeHash::Empty,
            original_code_hash: H256::from(code_id.into_bytes()).into(),
            instrumented_code_hash: instrumented_code_hash.into(),
            gas_reservation_map_hash: MaybeHash::Empty,
            memory_infix: Default::default(),
            balance: 0,
        };

        let journal = processor.run_one(program_id, &program_state);
        for note in journal {
            match note {
                JournalNote::SendDispatch { dispatch, .. } => {
                    assert_eq!(
                        String::from_utf8(dispatch.message().payload_bytes().to_vec()),
                        Ok("PONG".to_string())
                    );
                    return;
                }
                _ => (),
            }
        }
        panic!("No SendDispatch note found");
    }
}
