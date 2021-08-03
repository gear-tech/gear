use gear_core::{
    message::Message,
    program::{Program, ProgramId},
    storage::{
        InMemoryMessageQueue, InMemoryProgramStorage, MessageQueue, ProgramStorage, Storage,
    },
};
use gear_core_runner::runner::{Config, Runner};
use gear_node_rti::ext::{ExtMessageQueue, ExtProgramStorage};
use crate::sample::{PayloadVariant, Test};
use std::fmt::Write;

use regex::Regex;

fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).expect("Format failed")
    }
    s
}

const SOME_FIXED_USER: u64 = 1000001;

pub trait CollectState {
    fn collect(self) -> FinalState;
}

impl CollectState for Storage<InMemoryMessageQueue, InMemoryProgramStorage> {
    fn collect(self) -> FinalState {
        let message_queue = self.message_queue;
        let program_storage = self.program_storage;

        FinalState {
            log: message_queue.log().to_vec(),
            messages: message_queue.drain(),
            program_storage: program_storage.drain(),
        }
    }
}

impl CollectState for Storage<ExtMessageQueue, ExtProgramStorage> {
    fn collect(self) -> FinalState {
        let log = self.message_queue.log;

        let mut messages = Vec::new();

        let mut message_queue =
            common::storage_queue::StorageQueue::get("g::msg::".as_bytes().to_vec());
        while let Some(message) = message_queue.dequeue() {
            messages.push(message);
        }

        FinalState {
            log,
            messages,
            // TODO: iterate program storage to list programs here
            program_storage: Vec::new(),
        }
    }
}

pub fn init_fixture<MQ: MessageQueue, PS: ProgramStorage>(
    storage: Storage<MQ, PS>,
    test: &Test,
    fixture_no: usize,
) -> anyhow::Result<Runner<MQ, PS>> {
    let mut runner = Runner::new(&Config::default(), storage);
    let mut nonce = 0;
    for program in test.programs.iter() {
        let code = std::fs::read(program.path.clone())?;
        let mut init_message = Vec::new();
        if let Some(init_msg) = &program.init_message {
            let re = Regex::new(r"\{(?P<id>[0-9]*)\}").unwrap();
            init_message = match init_msg {
                PayloadVariant::Utf8(s) => {
                    // Insert ProgramId
                    if let Some(caps) = re.captures(s) {
                        let id = caps["id"].parse::<u64>().unwrap();
                        let s = s.replace(&caps[0], &encode_hex(ProgramId::from(id).as_slice()));
                        (s.clone().into_bytes()).to_vec()
                    } else {
                        init_msg.clone().into_raw()
                    }
                }
                _ => init_msg.clone().into_raw(),
            }
        }
        runner.init_program(
            SOME_FIXED_USER.into(),
            nonce,
            program.id.into(),
            code,
            init_message,
            program.init_gas_limit.unwrap_or(u64::MAX),
            program.init_value.unwrap_or(0) as _,
        )?;

        nonce += 1;
    }

    let fixture = &test.fixtures[fixture_no];
    for message in fixture.messages.iter() {
        let re = Regex::new(r"\{(?P<id>[0-9]*)\}").unwrap();
        let payload = match &message.payload {
            Some(PayloadVariant::Utf8(s)) => {
                // Insert ProgramId
                if let Some(caps) = re.captures(s) {
                    let id = caps["id"].parse::<u64>().unwrap();
                    let s = s.replace(&caps[0], &encode_hex(ProgramId::from(id).as_slice()));
                    (s.clone().into_bytes()).to_vec()
                } else {
                    message
                        .payload
                        .as_ref()
                        .expect("Checked above.")
                        .clone()
                        .into_raw()
                }
            }
            _ => message
                .payload
                .as_ref()
                .map(|payload| payload.clone().into_raw())
                .unwrap_or_default(),
        };
        runner.queue_message(
            0.into(),
            nonce,
            message.destination.into(),
            payload,
            message.gas_limit.unwrap_or(1000000000),
            message.value.unwrap_or_default() as _,
        );

        nonce += 1;
    }

    Ok(runner)
}

pub struct FinalState {
    pub messages: Vec<Message>,
    pub log: Vec<Message>,
    pub program_storage: Vec<Program>,
}

pub fn run<MQ: MessageQueue, PS: ProgramStorage>(
    mut runner: Runner<MQ, PS>,
    steps: Option<u64>,
) -> (FinalState, anyhow::Result<()>)
where
    Storage<MQ, PS>: CollectState,
{
    let mut result = Ok(());
    if let Some(steps) = steps {
        for step_no in 0..steps {
            if runner.run_next().traps > 0 && step_no + 1 == steps {
                result = Err(anyhow::anyhow!("Runner resulted in a trap"));
            }
        }
    } else {
        while runner.run_next().handled != 0 {}
    }

    let storage = runner.complete();

    (storage.collect(), result)
}
