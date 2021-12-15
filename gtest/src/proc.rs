
use crate::js::{MetaData, MetaType};
use crate::sample::{PayloadVariant, Test};
use gear_core::{
    message::{IncomingMessage, Message},
    program::{Program, ProgramId},
};
use std::time::{SystemTime, UNIX_EPOCH};

use sp_keyring::sr25519::Keyring;
use std::fmt::Write;
use std::str::FromStr;
use sp_core::{crypto::Ss58Codec, hexdisplay::AsBytesRef, sr25519::Public};

use regex::Regex;

use gear_core_processor::{
    configs::*,
    common::*,
    ext::Ext,
    handler,
    processor,
};

fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).expect("Format failed")
    }
    s
}

fn parse_payload(payload: String) -> String {
    let program_id_regex = Regex::new(r"\{(?P<id>[0-9]+)\}").unwrap();
    let account_regex = Regex::new(r"\{(?P<id>[a-z]+)\}").unwrap();
    let ss58_regex = Regex::new(r"\{(?P<id>[A-Za-z0-9]+)\}").unwrap();

    // Insert ProgramId
    let mut s = payload;
    while let Some(caps) = program_id_regex.captures(&s) {
        let id = caps["id"].parse::<u64>().unwrap();
        s = s.replace(&caps[0], &encode_hex(ProgramId::from(id).as_slice()));
    }

    while let Some(caps) = account_regex.captures(&s) {
        let id = &caps["id"];
        s = s.replace(
            &caps[0],
            &encode_hex(
                ProgramId::from_slice(Keyring::from_str(id).unwrap().to_h256_public().as_bytes())
                    .as_slice(),
            ),
        );
    }

    while let Some(caps) = ss58_regex.captures(&s) {
        let id = &caps["id"];
        s = s.replace(
            &caps[0],
            &encode_hex(
                ProgramId::from_slice(Public::from_ss58check(id).unwrap().as_bytes_ref())
                    .as_slice(),
            ),
        );
    }

    s
}

const SOME_FIXED_USER: u64 = 1000001;

#[derive(Clone, Debug)]
pub struct InitMessage {
    pub id: ProgramId,
    pub code: Vec<u8>,
    pub message: IncomingMessage,
}

impl From<InitMessage> for Dispatch {
    fn from(other: InitMessage) -> Self {
        Self {
            kind: DispatchKind::Init,
            message: other.message.into_message(other.id),
        }
    }
}

pub fn message_to_dispatch(message: Message) -> Dispatch {
    Dispatch {
        kind: if message.reply().is_none() {
            DispatchKind::Handle
        } else {
            DispatchKind::HandleReply
        },
        message,
    }
}

pub fn init_program(
    message: InitMessage,
    block_info: BlockInfo,
    journal_handler: &mut dyn JournalHandler
) -> anyhow::Result<()> {
    let program = Program::new(message.id, message.code.clone(), Default::default())?;

    if program.static_pages() > AllocationsConfig::default().max_pages.raw() {
        return Err(anyhow::anyhow!("Error initialisation: memory limit exceeded"));
    }

    let res = processor::process::<gear_backend_wasmtime::WasmtimeEnvironment::<Ext>>(program, message.into(), block_info);

    handler::handle_journal(res.journal, journal_handler);

    Ok(())
}

pub fn init_fixture(
    test: &Test,
    fixture_no: usize,
    journal_handler: &mut dyn JournalHandler,
) -> anyhow::Result<()> {
    let mut nonce = 0;

    for program in &test.programs {
        let program_path = program.path.clone();
        let code = std::fs::read(&program_path)?;
        let mut init_message = Vec::new();
        if let Some(init_msg) = &program.init_message {
            init_message = match init_msg {
                PayloadVariant::Utf8(s) => parse_payload(s.clone()).into_bytes(),
                PayloadVariant::Custom(v) => {
                    let meta_type = MetaType::InitInput;

                    let payload =
                        parse_payload(serde_json::to_string(&v).expect("Cannot convert to string"));

                    let json = MetaData::Json(payload);

                    let wasm = program_path.replace(".wasm", ".meta.wasm");

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

        let message_id = nonce.into();
        let id = program.id.to_program_id();

        let _ = init_program(
            InitMessage {
                id,
                code,
                message: IncomingMessage::new(
                    message_id,
                    init_source,
                    init_message.into(),
                    program.init_gas_limit.unwrap_or(u64::MAX),
                    program.init_value.unwrap_or(0) as u128,
                ),
            },
            Default::default(),
            journal_handler
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

                let wasm = test
                    .programs
                    .iter()
                    .filter(|p| p.id == message.destination)
                    .last()
                    .expect("Program not found")
                    .path
                    .clone()
                    .replace(".wasm", ".meta.wasm");

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

        let mut message_source: ProgramId = SOME_FIXED_USER.into();
        if let Some(source) = &message.source {
            message_source = source.to_program_id();
        }

        journal_handler.send_message(
            Default::default(),
            Message {
                id: nonce.into(),
                source: message_source,
                dest: message.destination.to_program_id(),
                payload: payload.into(),
                gas_limit: message.gas_limit.unwrap_or(u64::MAX),
                value: message.value.unwrap_or_default() as _,
                reply: None,
            }
        );

        nonce += 1;
    }

    Ok(())
}

pub fn run<JH>(
    steps: Option<usize>,
    journal_handler: &mut JH,
) -> Vec<(State, anyhow::Result<()>)>
where JH: JournalHandler + CollectState {
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

            if let Some(m) = state.message_queue.pop_front() {
                let program = state.programs
                    .get(&m.dest())
                    .expect("Can't find program");

                let res = processor::process::<gear_backend_wasmtime::WasmtimeEnvironment::<Ext>>(program.clone(), message_to_dispatch(m), BlockInfo { height, timestamp });

                handler::handle_journal(res.journal, journal_handler);

                log::debug!("step: {}", step_no + 1);
            }

            state = journal_handler.collect();
            results.push((state.clone(), Ok(())));
        }
    } else {
        let mut counter = 0;
        while let Some(m) = state.message_queue.pop_front() {
            let program = state.programs
                    .get(&m.dest())
                    .expect("Can't find program");

            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0) as u64;

            let res = processor::process::<gear_backend_wasmtime::WasmtimeEnvironment::<Ext>>(program.clone(), message_to_dispatch(m), BlockInfo { height: counter, timestamp });
            counter += 1;

            handler::handle_journal(res.journal.clone(), journal_handler);
            state = journal_handler.collect();
            results.push((state.clone(), Ok(())));
        }
    }

    results
}
