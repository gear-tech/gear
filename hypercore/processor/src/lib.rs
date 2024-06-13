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
use core_processor::common::JournalNote;
use gear_core::{
    ids::{ActorId, MessageId, ProgramId},
    message::DispatchKind,
};
use gprimitives::{CodeId, H256};
use host::InstanceWrapper;
use hypercore_observer::Event;
use parity_scale_codec::{Decode, Encode};
use std::collections::BTreeMap;

pub use db::Database;

pub(crate) mod db;
pub mod host;
mod run;

pub struct Processor {
    db: Database,
}

/// Local changes that can be committed to the network or local signer.
#[derive(Debug, Encode, Decode)]
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
                .write_instrumented_code(hypercore_runtime::VERSION, code_id, instrumented);

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn run_on_host(
        &mut self,
        program_id: ProgramId,
        program_state: H256,
    ) -> Result<Vec<JournalNote>> {
        let original_code_id = self.db.get_program_code_id(program_id).unwrap();

        let maybe_instrumented_code = self
            .db
            .read_instrumented_code(hypercore_runtime::VERSION, original_code_id);

        let mut instance_wrapper = host::InstanceWrapper::new(self.db.clone())?;

        instance_wrapper.run(
            program_id,
            original_code_id,
            program_state,
            maybe_instrumented_code,
        )
    }

    // TODO: use proper `Dispatch` type here instead of db's.
    pub fn run(
        &mut self,
        programs: BTreeMap<ProgramId, H256>,
        messages: BTreeMap<ProgramId, Vec<UserMessage>>,
    ) -> Result<()> {
        let mut programs = programs;
        let _messages_to_users = run::run(8, self.db.clone(), &mut programs, messages);
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

pub struct UserMessage {
    id: MessageId,
    kind: DispatchKind,
    source: ActorId,
    payload: Vec<u8>,
    gas_limit: u64,
    value: u128,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gear_core::{
        ids::prelude::CodeIdExt,
        message::{DispatchKind, Payload},
        program::ProgramState as InitStatus,
    };
    use gprimitives::{ActorId, MessageId};
    use hypercore_db::MemDb;
    use hypercore_runtime_common::state::{self, Dispatch, MaybeHash, ProgramState, Storage};
    use std::collections::VecDeque;
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
        let instrumented = processor
            .db
            .read_instrumented_code(hypercore_runtime::VERSION, id)
            .unwrap();

        assert_eq!(instrumented.original_code_len() as usize, code_len);
        assert!(instrumented.code().len() > code_len);
    }

    #[test]
    fn host_ping_pong() {
        init_logger();

        let db = MemDb::default();
        let mut processor = Processor::new(Database::from_one(&db));

        let program_id = 42.into();

        let code_id = upload_code(&mut processor, demo_ping::WASM_BINARY).unwrap();

        let state_hash = create_program(
            &mut processor,
            program_id,
            code_id,
            create_message(DispatchKind::Init, "PING"),
        )
        .unwrap();

        let _init = processor.run_on_host(program_id, state_hash).unwrap();
    }

    #[test]
    fn ping_pong() {
        init_logger();

        let db = MemDb::default();
        let mut processor = Processor::new(Database::from_one(&db));

        let user_id = ActorId::from(10);
        let program_id = ProgramId::from(0x10000);

        let code_id = upload_code(&mut processor, demo_ping::WASM_BINARY).unwrap();

        assert!(processor.instrument_code(code_id).unwrap());

        let state_hash = create_program(
            &mut processor,
            program_id,
            code_id,
            UserMessage {
                id: MessageId::from(1),
                kind: DispatchKind::Init,
                source: user_id,
                payload: b"PING".to_vec(),
                gas_limit: 1_000_000_000,
                value: 1,
            },
        )
        .unwrap();

        let mut programs = BTreeMap::from_iter([(program_id, state_hash)]);

        send_user_message(
            &mut processor,
            &mut programs,
            program_id,
            UserMessage {
                id: MessageId::from(2),
                kind: DispatchKind::Handle,
                source: user_id,
                payload: b"PING".to_vec(),
                gas_limit: 1_000_000_000,
                value: 1,
            },
        );

        let to_users = run::run(8, processor.db.clone(), &mut programs, Default::default());

        assert_eq!(to_users.len(), 2);

        let message = &to_users[0];
        assert_eq!(message.destination(), user_id);
        assert_eq!(message.payload_bytes(), b"PONG");

        let message = &to_users[1];
        assert_eq!(message.destination(), user_id);
        assert_eq!(message.payload_bytes(), b"PONG");
    }

    fn upload_code(processor: &mut Processor, code: &[u8]) -> Result<CodeId> {
        let code_id = CodeId::generate(code);
        assert!(processor.new_code(code.to_vec()).unwrap());
        assert!(processor.instrument_code(code_id).unwrap());
        Ok(code_id)
    }

    fn create_message(kind: DispatchKind, payload: impl AsRef<[u8]>) -> UserMessage {
        UserMessage {
            id: H256::random().0.into(),
            kind,
            source: H256::random().0.into(),
            payload: payload.as_ref().to_vec(),
            gas_limit: 1_000_000_000,
            value: 0,
        }
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

    fn send_user_message(
        processor: &mut Processor,
        programs: &mut BTreeMap<ProgramId, H256>,
        program_id: ProgramId,
        message: UserMessage,
    ) {
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

        let mut program_state = processor.db.read_state(programs[&program_id]).unwrap();
        let mut queue = program_state
            .queue_hash
            .with_hash_or_default(|hash| processor.db.read_queue(hash).unwrap());
        queue.push_back(dispatch);
        log::info!("Process queue after send message: {queue:?}");
        let queue_hash = processor.db.write_queue(queue);
        program_state.queue_hash = queue_hash.into();
        let new_state_hash = processor.db.write_state(program_state);
        programs.insert(program_id, new_state_hash);
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
            .collect::<BTreeMap<_, _>>();

        let wait_for_reply_to = get_next_message_id();
        send_user_message(
            &mut processor,
            &mut programs,
            async_id,
            UserMessage {
                id: wait_for_reply_to,
                kind: DispatchKind::Handle,
                source: user_id,
                payload: demo_async::Command::Common.encode(),
                gas_limit: 10_000_000_000,
                value: 0,
            },
        );

        let to_users = run::run(8, processor.db.clone(), &mut programs, Default::default());

        assert_eq!(to_users.len(), 3);

        let message = &to_users[0];
        assert_eq!(message.destination(), user_id);
        assert_eq!(message.payload_bytes(), b"PONG");

        let message = &to_users[1];
        assert_eq!(message.destination(), user_id);
        assert_eq!(message.payload_bytes(), b"");

        let message = &to_users[2];
        assert_eq!(message.destination(), user_id);
        assert_eq!(
            message.payload_bytes(),
            wait_for_reply_to.into_bytes().as_slice()
        );
    }
}
