// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::address::Address;
use crate::check::ExecutionContext;
use crate::js::{MetaData, MetaType};
use crate::sample::{PayloadVariant, Test};
use core_processor::{common::*, configs::*, Ext};
use gear_backend_common::Environment;
use gear_core::{
    code::{CheckedCode, InstrumentedCode},
    ids::{CodeId, MessageId, ProgramId},
    message::{Dispatch, DispatchKind, IncomingDispatch, IncomingMessage, Message},
};
use regex::Regex;
use std::{
    io::Error as IoError,
    io::ErrorKind as IoErrorKind,
    time::{SystemTime, UNIX_EPOCH},
};

use frame_system as system;
use gear_runtime::{Runtime, System};

pub const EXISTENTIAL_DEPOSIT: u128 = 500;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let t = system::GenesisConfig::default()
        .build_storage::<Runtime>()
        .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub fn parse_payload(payload: String) -> String {
    let program_id_regex = Regex::new(r"\{(?P<id>[0-9]+)\}").unwrap();
    let account_regex = Regex::new(r"\{(?P<id>[a-z]+)\}").unwrap();

    // Insert ProgramId
    let mut s = payload;
    while let Some(caps) = program_id_regex.captures(&s) {
        let id = caps["id"].parse::<u64>().unwrap();
        s = s.replace(&caps[0], &hex::encode(ProgramId::from(id).as_ref()));
    }

    while let Some(caps) = account_regex.captures(&s) {
        let id = &caps["id"];
        s = s.replace(
            &caps[0],
            &hex::encode(Address::Account(id.to_string()).to_program_id().as_ref()),
        );
    }

    s
}

pub const SOME_FIXED_USER: u64 = 1000001;
pub const GAS_LIMIT: u64 = 100_000_000_000;

#[derive(Clone, Debug)]
pub struct InitMessage {
    pub id: ProgramId,
    pub code: Vec<u8>,
    pub message: IncomingMessage,
}

impl From<InitMessage> for IncomingDispatch {
    fn from(other: InitMessage) -> Self {
        IncomingDispatch::new(DispatchKind::Init, other.message, None)
    }
}

pub fn init_program<E, JH>(
    message: InitMessage,
    block_info: BlockInfo,
    journal_handler: &mut JH,
) -> anyhow::Result<()>
where
    E: Environment<Ext>,
    JH: JournalHandler + CollectState + ExecutionContext,
{
    let code = CheckedCode::try_new(message.code.clone())?;

    if code.static_pages() > AllocationsConfig::default().max_pages.raw() {
        return Err(anyhow::anyhow!(
            "Error initialisation: memory limit exceeded"
        ));
    }

    let program = journal_handler.store_program(message.id, code, message.message.id());
    let program_id = program.id();
    journal_handler.write_gas(message.message.id(), message.message.gas_limit());

    let journal = core_processor::process::<Ext, E>(
        Some(ExecutableActor {
            program,
            balance: 0,
        }),
        message.into(),
        block_info,
        EXISTENTIAL_DEPOSIT,
        Default::default(),
        program_id,
    );

    core_processor::handle_journal(journal, journal_handler);

    Ok(())
}

pub fn init_fixture<E, JH>(
    test: &Test,
    fixture_no: usize,
    journal_handler: &mut JH,
) -> anyhow::Result<()>
where
    E: Environment<Ext>,
    JH: JournalHandler + CollectState + ExecutionContext,
{
    let mut nonce = 1;

    if let Some(codes) = &test.codes {
        for code in codes {
            let code_bytes = std::fs::read(&code.path)
                .map_err(|e| IoError::new(IoErrorKind::Other, format!("`{}': {}", code.path, e)))?;
            if let Ok(code) = CheckedCode::try_new(code_bytes) {
                let code_hash = CodeId::generate(code.code());
                journal_handler.store_original_code(code.clone());
                let instrumented_code: Vec<u8> = new_test_ext().execute_with(|| {
                    let schedule = <Runtime as pallet_gear::pallet::Config>::Schedule::get();

                    let module = wasm_instrument::parity_wasm::deserialize_buffer(code.code())
                        .unwrap_or_default();

                    let gas_rules = schedule.rules(&module);

                    let instrumented_module = wasm_instrument::gas_metering::inject(
                        module, &gas_rules, "env",
                    )
                    .map_err(|_| anyhow::anyhow!("Error initialisation: memory limit exceeded"))?;

                    wasm_instrument::parity_wasm::elements::serialize(instrumented_module)
                        .map_err(|_| anyhow::anyhow!("Error initialisation: memory limit exceeded"))
                })?;
                journal_handler.store_code(
                    code_hash,
                    InstrumentedCode::new(instrumented_code, code.static_pages(), 1),
                );
            }
        }
    }

    for program in &test.programs {
        let program_path = program.path.clone();
        let code = std::fs::read(&program_path)
            .map_err(|e| IoError::new(IoErrorKind::Other, format!("`{}': {}", program_path, e)))?;
        let mut init_message = Vec::new();
        if let Some(init_msg) = &program.init_message {
            init_message = match init_msg {
                PayloadVariant::Utf8(s) => parse_payload(s.clone()).into_bytes(),
                PayloadVariant::Custom(v) => {
                    let meta_type = MetaType::InitInput;

                    let payload =
                        parse_payload(serde_json::to_string(&v).expect("Cannot convert to string"));

                    let json = MetaData::Json(payload);

                    let wasm = crate::sample::get_meta_wasm_path(program_path);

                    json.convert(&wasm, &meta_type)
                        .expect("Unable to get bytes")
                        .into_bytes()
                }
                _ => init_msg.clone().into_raw(),
            }
        }
        let mut init_source: ProgramId = SOME_FIXED_USER.into();
        if let Some(source) = &program.source {
            init_source = source.to_program_id();
        }

        let message_id = MessageId::from(nonce);
        let id = program.id.to_program_id();

        let _ = init_program::<E, JH>(
            InitMessage {
                id,
                code,
                message: IncomingMessage::new(
                    message_id,
                    init_source,
                    init_message,
                    program.init_gas_limit.unwrap_or(GAS_LIMIT),
                    program.init_value.unwrap_or(0) as u128,
                    None,
                ),
            },
            Default::default(),
            journal_handler,
        )?;

        nonce += 1;
    }

    let fixture = &test.fixtures[fixture_no];

    for message in &fixture.messages {
        let payload = match &message.payload {
            Some(PayloadVariant::Utf8(s)) => parse_payload(s.clone()).as_bytes().to_vec(),
            Some(PayloadVariant::Custom(v)) => {
                let meta_type = MetaType::HandleInput;

                let payload =
                    parse_payload(serde_json::to_string(&v).expect("Cannot convert to string"));

                let json = MetaData::Json(payload);

                let wasm = crate::sample::get_meta_wasm_path(
                    test.programs
                        .iter()
                        .filter(|p| p.id == message.destination)
                        .last()
                        .expect("Program not found")
                        .path
                        .clone(),
                );
                json.convert(&wasm, &meta_type)
                    .expect("Unable to get bytes")
                    .into_bytes()
            }
            _ => message
                .payload
                .as_ref()
                .map(|payload| payload.clone().into_raw())
                .unwrap_or_default(),
        };

        let message_id = MessageId::from(nonce);
        let gas_limit = message.gas_limit.unwrap_or(GAS_LIMIT);

        let mut message_source: ProgramId = SOME_FIXED_USER.into();
        if let Some(source) = &message.source {
            message_source = source.to_program_id();
        }

        let message = Message::new(
            message_id,
            message_source,
            message.destination.to_program_id(),
            payload,
            Some(gas_limit),
            message.value.unwrap_or_default() as _,
            None,
        );
        let dispatch = Dispatch::new(DispatchKind::Handle, message);

        journal_handler.send_dispatch(Default::default(), dispatch);

        nonce += 1;
    }

    Ok(())
}

pub fn run<JH, E>(
    steps: Option<usize>,
    journal_handler: &mut JH,
) -> Vec<(State, anyhow::Result<()>)>
where
    JH: JournalHandler + CollectState + ExecutionContext,
    E: Environment<Ext>,
{
    let mut results = Vec::new();
    let mut state = journal_handler.collect();
    results.push((state.clone(), Ok(())));

    if let Some(steps) = steps {
        for step_no in 0..steps {
            let height = step_no as u32;
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0) as u64;

            if let Some((dispatch, gas_limit)) = state.dispatch_queue.pop_front() {
                let actor = state.actors.get(&dispatch.destination()).cloned();

                let program_id = dispatch.destination();

                let journal = core_processor::process::<Ext, E>(
                    actor,
                    dispatch.into_incoming(gas_limit),
                    BlockInfo { height, timestamp },
                    EXISTENTIAL_DEPOSIT,
                    Default::default(),
                    program_id,
                );

                core_processor::handle_journal(journal, journal_handler);

                log::debug!("step: {}", step_no + 1);
            }

            state = journal_handler.collect();
            log::debug!("{:?}", state);
            results.push((state.clone(), Ok(())));
        }
    } else {
        let mut counter = 0;
        while let Some((dispatch, gas_limit)) = state.dispatch_queue.pop_front() {
            let actor = state.actors.get(&dispatch.destination()).cloned();

            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0) as u64;

            let program_id = dispatch.destination();

            let journal = core_processor::process::<Ext, E>(
                actor,
                dispatch.into_incoming(gas_limit),
                BlockInfo {
                    height: counter,
                    timestamp,
                },
                EXISTENTIAL_DEPOSIT,
                Default::default(),
                program_id,
            );
            counter += 1;

            core_processor::handle_journal(journal, journal_handler);

            state = journal_handler.collect();
            log::debug!("{:?}", state);
            results.push((state.clone(), Ok(())));
        }
    }

    results
}
