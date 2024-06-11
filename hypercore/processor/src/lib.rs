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

use anyhow::{anyhow, Result};
use db::Storage;
use gear_core::{
    ids::{prelude::CodeIdExt as _, ProgramId},
    message::IncomingMessage,
};
use gprimitives::{CodeId, H256};
use host::InstanceWrapper;
use hypercore_observer::Event;
use std::collections::HashMap;

pub use db::Database;

pub(crate) mod db;
pub mod host;

const RUNTIME_ID: u32 = 0;

pub struct Processor {
    db: Database,
}

/// Local changes that can be committed to the network or local signer.
pub enum LocalOutcome {
    /// Produced when code with specific id is recorded and available in database.
    CodeCommitment(CodeId),
}

impl Processor {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn new_code(&mut self, code: Vec<u8>) -> Result<bool> {
        let mut executor = InstanceWrapper::new(self.db.clone())?;

        let res = executor.verify(&code)?;

        if res {
            let _ = self.db.write_original_code(&code);
        }

        Ok(res)
    }

    pub fn instrument_code(&mut self, code_id: CodeId) -> Result<bool> {
        let code = self.db.read_original_code(code_id).unwrap();

        let mut instance_wrapper = host::InstanceWrapper::new(self.db.clone())?;

        if let Some(instrumented) = instance_wrapper.instrument(&code)? {
            self.db
                .write_instrumented_code(RUNTIME_ID, code_id, instrumented);

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn run_on_host(&mut self, code_id: CodeId) -> Result<()> {
        let instrumented_code = self
            .db
            .read_instrumented_code(RUNTIME_ID, code_id)
            .ok_or_else(|| anyhow!("couldn't find instrumented code"))?;

        let mut instance_wrapper = host::InstanceWrapper::new(self.db.clone())?;

        // TODO: set state_hash.
        instance_wrapper.run(Default::default(), &instrumented_code)?;

        Ok(())
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

    pub fn process_observer_event(&mut self, event: &Event) -> Result<Vec<LocalOutcome>> {
        match event {
            Event::UploadCode { code_id, code, .. } => {
                log::debug!("Processing upload code {code_id:?}");

                if self.new_code(code.to_vec())? {
                    Ok(vec![LocalOutcome::CodeCommitment(*code_id)])
                } else {
                    Ok(vec![])
                }
            }
            Event::Block {
                ref block_hash,
                events: _,
            } => {
                log::debug!("Processing events for {block_hash:?}");
                Ok(vec![])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque, os::unix::process, pin, result, thread::sleep, time::Duration,
    };

    use super::*;
    use gear_core::{
        message::{DispatchKind, MessageDetails, Payload},
        program::ProgramState as InitStatus,
    };
    use gprimitives::{ActorId, MessageId};
    use hypercore_db::MemDb;
    use hypercore_runtime_native::{
        hypercore_runtime_common::receipts::Receipt,
        process_program,
        state::{self, Dispatch, MaybeHash, ProgramState},
        NativeRuntimeInterface,
    };
    use parity_scale_codec::{Decode, Encode};
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
        let mut processor = Processor::new(Database::from_one(&db));

        let valid = valid_code();
        let valid_id = CodeId::generate(&valid);

        assert!(processor.db.read_original_code(valid_id).is_none());
        assert!(processor.new_code(valid).unwrap());
        assert!(processor.db.read_original_code(valid_id).is_some());

        let invalid = vec![0; 42];
        let invalid_id = CodeId::generate(&invalid);

        assert!(processor.db.read_original_code(invalid_id).is_none());
        assert!(!processor.new_code(invalid).unwrap());
        assert!(processor.db.read_original_code(invalid_id).is_none());
    }

    #[test]
    fn instrument_code() {
        init_logger();

        let db = MemDb::default();
        let mut processor = Processor::new(Database::from_one(&db));

        let code = valid_code();
        let code_len = code.len();
        let id = CodeId::generate(&code);

        assert!(processor.new_code(code).unwrap());

        assert!(processor.instrument_code(id).unwrap());
        let instrumented = processor.db.read_instrumented_code(RUNTIME_ID, id).unwrap();

        assert_eq!(instrumented.original_code_len() as usize, code_len);
        assert!(instrumented.code().len() > code_len);
    }

    #[test]
    fn host_sandbox() {
        init_logger();

        let db = MemDb::default();
        let mut processor = Processor::new(Database::from_one(&db));

        let code = valid_code();
        let code_id = CodeId::generate(&code);

        assert!(processor.new_code(code).unwrap());

        assert!(processor.instrument_code(code_id).unwrap());
        let _instrumented = processor
            .db
            .read_instrumented_code(RUNTIME_ID, code_id)
            .unwrap();

        processor.run_on_host(code_id).unwrap();
    }

    #[test]
    fn ping_pong() {
        init_logger();

        let db = MemDb::default();
        let mut processor = Processor::new(Database::from_one(&db));

        let program_id = ProgramId::default();

        let code = demo_ping::WASM_BINARY;
        let code_id = CodeId::generate(code);
        assert!(processor.new_code(code.to_vec()).unwrap());
        processor.db.set_program_code_id(program_id, code_id);

        assert!(processor.instrument_code(code_id).unwrap());

        let payload = Payload::try_from(b"PING".to_vec()).unwrap();
        let payload_hash = processor.db.write_payload(payload);

        let init_dispatch = Dispatch {
            id: MessageId::from(1),
            kind: DispatchKind::Init,
            source: ActorId::from(10),
            payload_hash: payload_hash.into(),
            gas_limit: 1_000_000_000,
            value: 1,
            details: None,
            context: None,
        };

        let dispatch = Dispatch {
            id: MessageId::from(2),
            kind: DispatchKind::Handle,
            source: ActorId::from(20),
            payload_hash: payload_hash.into(),
            gas_limit: 1_000_000_000,
            value: 1,
            details: None,
            context: None,
        };

        // TODO: queue is vec so init dispatch is after handle
        let queue = VecDeque::from(vec![init_dispatch, dispatch]);
        let queue_hash = processor.db.write_queue(queue);

        let active_program = state::ActiveProgram {
            allocations_hash: MaybeHash::Empty,
            pages_hash: MaybeHash::Empty,
            gas_reservation_map_hash: MaybeHash::Empty,
            memory_infix: Default::default(),
            status: InitStatus::Uninitialized {
                message_id: MessageId::from(1),
            },
        };

        let program_state = ProgramState {
            state: state::Program::Active(active_program),
            queue_hash: queue_hash.into(),
            waitlist_hash: MaybeHash::Empty,
            balance: 0,
        };

        let instrumented_code = processor
            .db
            .read_instrumented_code(RUNTIME_ID, code_id)
            .unwrap();

        let ri = NativeRuntimeInterface::new(&processor.db, Default::default());
        let (_, receipts) = process_program(
            program_id,
            program_state,
            Some(instrumented_code),
            u32::MAX,
            u64::MAX,
            code_id,
            &ri,
        );

        let mut pongs_amount = 0;
        for receipt in receipts.into_iter() {
            if let Receipt::SendDispatch { dispatch, .. } = receipt {
                if String::from_utf8(dispatch.message().payload_bytes().to_vec())
                    == Ok("PONG".to_string())
                {
                    pongs_amount += 1;
                }
            }
        }
        assert_eq!(pongs_amount, 2);
    }

    struct UserMessage {
        id: MessageId,
        kind: DispatchKind,
        source: ActorId,
        payload: Vec<u8>,
        gas_limit: u64,
        value: u128,
    }

    fn upload_code(processor: &mut Processor, code: &[u8]) -> Result<CodeId> {
        let code_id = CodeId::generate(code);
        assert!(processor.new_code(code.to_vec()).unwrap());
        assert!(processor.instrument_code(code_id).unwrap());
        Ok(code_id)
    }

    fn create_program(
        processor: &mut Processor,
        program_id: ProgramId,
        code_id: CodeId,
        init_message: UserMessage,
    ) -> Result<H256> {
        assert_eq!(init_message.kind, DispatchKind::Init);

        processor.db.set_program_code_id(program_id, code_id);

        let payload_hash = match init_message.payload.len() {
            0 => MaybeHash::Empty,
            _ => processor
                .db
                .write_payload(Payload::try_from(init_message.payload.clone()).unwrap())
                .into(),
        };

        let init_dispatch = Dispatch {
            id: init_message.id,
            kind: DispatchKind::Init,
            source: init_message.source,
            payload_hash,
            gas_limit: init_message.gas_limit,
            value: init_message.value,
            details: None,
            context: None,
        };

        let queue = VecDeque::from(vec![init_dispatch]);
        let queue_hash = processor.db.write_queue(queue);

        let active_program = state::ActiveProgram {
            allocations_hash: MaybeHash::Empty,
            pages_hash: MaybeHash::Empty,
            gas_reservation_map_hash: MaybeHash::Empty,
            memory_infix: Default::default(),
            status: InitStatus::Uninitialized {
                message_id: init_message.id,
            },
        };

        let program_state = ProgramState {
            state: state::Program::Active(active_program),
            queue_hash: queue_hash.into(),
            waitlist_hash: MaybeHash::Empty,
            balance: 0,
        };

        Ok(processor.db.write_state(program_state))
    }

    fn process_programs(
        processor: &mut Processor,
        programs: &mut HashMap<ProgramId, H256>,
        mut messages: HashMap<ProgramId, Vec<UserMessage>>,
    ) -> Result<VecDeque<Receipt>> {
        let mut receipts = VecDeque::new();
        for (program_id, state_hash) in programs.clone().into_iter() {
            let messages = messages.remove(&program_id).unwrap_or_default();
            let mut program_state = processor.db.read_state(state_hash).unwrap();

            let mut queue = program_state
                .queue_hash
                .with_hash_or_default(|hash| processor.db.read_queue(hash).unwrap_or_default());

            for message in messages.into_iter() {
                let payload_hash = match message.payload.len() {
                    0 => MaybeHash::Empty,
                    _ => processor
                        .db
                        .write_payload(Payload::try_from(message.payload).unwrap())
                        .into(),
                };

                let dispatch = Dispatch {
                    id: message.id,
                    kind: message.kind,
                    source: message.source,
                    payload_hash,
                    gas_limit: message.gas_limit,
                    value: message.value,
                    details: None,
                    context: None,
                };

                queue.push_back(dispatch);
            }

            if !queue.is_empty() {
                let queue_hash = processor.db.write_queue(queue);
                program_state.queue_hash = queue_hash.into();
            }

            let code_id = processor
                .db
                .get_program_code_id(program_id)
                .expect("Code ID must be set");
            let instrumented_code = match &program_state.state {
                state::Program::Active(_) => Some(
                    processor
                        .db
                        .read_instrumented_code(RUNTIME_ID, code_id)
                        .expect("Instrumented code must be set at this point"),
                ),
                state::Program::Exited(_) | state::Program::Terminated(_) => None,
            };

            let ri = NativeRuntimeInterface::new(&processor.db, Default::default());
            let (new_state, new_receipts) = process_program(
                program_id,
                program_state,
                instrumented_code,
                u32::MAX,
                u64::MAX,
                code_id,
                &ri,
            );

            receipts.append(&mut new_receipts.into());

            programs.insert(program_id, processor.db.write_state(new_state));
        }
        Ok(receipts)
    }

    fn process_receipts(
        processor: &mut Processor,
        programs: &mut HashMap<ProgramId, H256>,
        receipts: VecDeque<Receipt>,
    ) {
        for receipt in receipts.into_iter() {
            match receipt {
                Receipt::SendDispatch { dispatch, .. } => {
                    let program_id = dispatch.message().destination();
                    if !programs.contains_key(&program_id) {
                        log::trace!("Message to user {program_id} was sent: {dispatch:?}");
                        continue;
                    }
                    let payload = dispatch.message().payload_bytes();
                    let payload_hash = payload
                        .is_empty()
                        .then_some(MaybeHash::Empty)
                        .unwrap_or_else(|| {
                            processor
                                .db
                                .write_payload(Payload::try_from(payload.to_vec()).unwrap())
                                .into()
                        });
                    let details = dispatch.reply_details().map(MessageDetails::Reply);

                    // TODO: temporary, gasless messages are not supported currently.
                    let gas_limit = dispatch.message().gas_limit().unwrap_or(100_000_000_000);

                    let dispatch = Dispatch {
                        id: dispatch.message().id(),
                        kind: dispatch.kind(),
                        source: dispatch.message().source(),
                        payload_hash,
                        gas_limit,
                        value: dispatch.message().value(),
                        details,
                        context: None,
                    };
                    let mut program_state = processor.db.read_state(programs[&program_id]).unwrap();
                    let mut queue = program_state
                        .queue_hash
                        .with_hash_or_default(|hash| processor.db.read_queue(hash).unwrap());
                    queue.push_back(dispatch);
                    let queue_hash = processor.db.write_queue(queue);
                    program_state.queue_hash = queue_hash.into();
                    let new_state_hash = processor.db.write_state(program_state);
                    programs.insert(program_id, new_state_hash);
                }
                r => todo!("Implement receipt {r:?} processing"),
            }
        }
    }

    #[test]
    fn async_and_ping() {
        init_logger();

        let mut message_nonce: u64 = 0;
        let mut get_next_message_id = || {
            message_nonce += 1;
            MessageId::from(message_nonce)
        };
        let user_id = ActorId::from(10);

        let db = MemDb::default();
        let mut processor = Processor::new(Database::from_one(&db));

        let ping_id = ProgramId::from(0x10000000);
        let async_id = ProgramId::from(0x20000000);

        let ping_code_id = upload_code(&mut processor, demo_ping::WASM_BINARY).unwrap();
        let upload_code_id = upload_code(&mut processor, demo_async::WASM_BINARY).unwrap();

        let ping_state_hash = create_program(
            &mut processor,
            ping_id,
            ping_code_id,
            UserMessage {
                id: get_next_message_id(),
                kind: DispatchKind::Init,
                source: user_id,
                payload: b"PING".to_vec(),
                gas_limit: 1_000_000_000,
                value: 0,
            },
        )
        .unwrap();

        let async_state_hash = create_program(
            &mut processor,
            async_id,
            upload_code_id,
            UserMessage {
                id: get_next_message_id(),
                kind: DispatchKind::Init,
                source: user_id,
                payload: ping_id.encode(),
                gas_limit: 1_000_000_000,
                value: 0,
            },
        )
        .unwrap();

        let mut programs = vec![(ping_id, ping_state_hash), (async_id, async_state_hash)]
            .into_iter()
            .collect::<HashMap<_, _>>();

        let receipts = process_programs(&mut processor, &mut programs, Default::default()).unwrap();
        process_receipts(&mut processor, &mut programs, receipts);

        let message_to_wait_for = get_next_message_id();
        for i in 1..10 {
            let messages = if i == 1 {
                let ms = vec![UserMessage {
                    id: message_to_wait_for,
                    kind: DispatchKind::Handle,
                    source: user_id,
                    payload: demo_async::Command::Common.encode(),
                    gas_limit: 10_000_000_000,
                    value: 0,
                }];
                vec![(async_id, ms)].into_iter().collect()
            } else {
                Default::default()
            };
            let receipts = process_programs(&mut processor, &mut programs, messages).unwrap();
            for r in receipts.iter() {
                if let Receipt::SendDispatch { dispatch, .. } = r {
                    if dispatch.destination() != user_id {
                        continue;
                    }
                    let reply_payload =
                        MessageId::decode(&mut dispatch.message().payload_bytes()).unwrap();
                    assert_eq!(message_to_wait_for, reply_payload);
                    let reply_for = dispatch.message().reply_details().unwrap().to_message_id();
                    assert_eq!(message_to_wait_for, reply_for);
                }
            }
            process_receipts(&mut processor, &mut programs, receipts);
        }
    }
}
