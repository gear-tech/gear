use crate::check::ExecutionContext;
use crate::js::{MetaData, MetaType};
use crate::sample::{PayloadVariant, Test};
use core_processor::{common::*, configs::*, Ext};
use gear_backend_common::Environment;
use gear_core::{
    message::{Dispatch, DispatchKind, IncomingMessage, Message, MessageId},
    program::{Program, ProgramId},
};
use regex::Regex;
use sp_core::{crypto::Ss58Codec, hexdisplay::AsBytesRef, sr25519::Public};
use sp_keyring::sr25519::Keyring;
use std::{
    io::Error as IoError,
    io::ErrorKind as IoErrorKind,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn parse_payload(payload: String) -> String {
    let program_id_regex = Regex::new(r"\{(?P<id>[0-9]+)\}").unwrap();
    let account_regex = Regex::new(r"\{(?P<id>[a-z]+)\}").unwrap();
    let ss58_regex = Regex::new(r"\{(?P<id>[A-Za-z0-9]+)\}").unwrap();

    // Insert ProgramId
    let mut s = payload;
    while let Some(caps) = program_id_regex.captures(&s) {
        let id = caps["id"].parse::<u64>().unwrap();
        s = s.replace(&caps[0], &hex::encode(ProgramId::from(id).as_slice()));
    }

    while let Some(caps) = account_regex.captures(&s) {
        let id = &caps["id"];
        s = s.replace(
            &caps[0],
            &hex::encode(
                ProgramId::from_slice(Keyring::from_str(id).unwrap().to_h256_public().as_bytes())
                    .as_slice(),
            ),
        );
    }

    while let Some(caps) = ss58_regex.captures(&s) {
        let id = &caps["id"];
        s = s.replace(
            &caps[0],
            &hex::encode(
                ProgramId::from_slice(Public::from_ss58check(id).unwrap().as_bytes_ref())
                    .as_slice(),
            ),
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

impl From<InitMessage> for Dispatch {
    fn from(other: InitMessage) -> Self {
        Self {
            kind: DispatchKind::Init,
            message: other.message.into_message(other.id),
        }
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
    let program = Program::new(message.id, message.code.clone())?;

    if program.static_pages() > AllocationsConfig::default().max_pages.raw() {
        return Err(anyhow::anyhow!(
            "Error initialisation: memory limit exceeded"
        ));
    }

    journal_handler.store_program(program.clone(), message.message.id());

    let journal = core_processor::process::<E>(Some(program), message.into(), block_info);

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

        let message_id = MessageId::from(nonce);
        let id = program.id.to_program_id();

        let _ = init_program::<E, JH>(
            InitMessage {
                id,
                code,
                message: IncomingMessage::new(
                    message_id,
                    init_source,
                    init_message.into(),
                    program.init_gas_limit.unwrap_or(GAS_LIMIT),
                    program.init_value.unwrap_or(0) as u128,
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

        let message_id = MessageId::from(nonce);
        let gas_limit = message.gas_limit.unwrap_or(GAS_LIMIT);

        let mut message_source: ProgramId = SOME_FIXED_USER.into();
        if let Some(source) = &message.source {
            message_source = source.to_program_id();
        }

        let message = Message {
            id: message_id,
            source: message_source,
            dest: message.destination.to_program_id(),
            payload: payload.into(),
            gas_limit,
            value: message.value.unwrap_or_default() as _,
            reply: None,
        };
        journal_handler.send_dispatch(Default::default(), Dispatch::new_handle(message));

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

            if let Some(dispatch) = state.dispatch_queue.pop_front() {
                let program = state.programs.get(&dispatch.message.dest()).cloned();

                let journal = core_processor::process::<E>(
                    program,
                    dispatch,
                    BlockInfo { height, timestamp },
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
        while let Some(dispatch) = state.dispatch_queue.pop_front() {
            let program = state.programs.get(&dispatch.message.dest()).cloned();

            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0) as u64;

            let journal = core_processor::process::<E>(
                program,
                dispatch,
                BlockInfo {
                    height: counter,
                    timestamp,
                },
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
