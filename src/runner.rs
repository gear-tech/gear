use gear_core::{
    memory::PageNumber,
    message::Message,
    program::{Program, ProgramId},
    runner::{Config, Runner},
    storage::{
        new_in_memory, InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage,
        InMemoryStorage,
    },
};
use std::{fmt::Write, num::ParseIntError};
use test_gear_sample::sample::{PayloadVariant, Test};

use regex::Regex;

type InMemoryRunner =
    Runner<InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage>;

fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b);
    }
    s
}

pub fn init_fixture(test: &Test, fixture_no: usize) -> anyhow::Result<InMemoryRunner> {
    let mut runner = Runner::new(
        &Config::default(),
        new_in_memory(Default::default(), Default::default(), Default::default()),
        &[],
    );
    for program in test.programs.iter() {
        let code = std::fs::read(program.path.clone())?;
        let mut init_message = Vec::new();
        if let Some(init_msg) = &program.init_message {
            let re = Regex::new(r"\{(?P<id>[0-9])*}").unwrap();
            init_message = match init_msg {
                PayloadVariant::Utf8(s) => {
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
            program.id.into(),
            code,
            init_message,
            program.init_gas_limit.unwrap_or(u64::MAX),
        )?;
    }

    let fixture = &test.fixtures[fixture_no];
    for message in fixture.messages.iter() {
        let re = Regex::new(r"\{(?P<id>[0-9])*}").unwrap();
        let payload = match &message.payload {
            PayloadVariant::Utf8(s) => {
                if let Some(caps) = re.captures(&s) {
                    let id = caps["id"].parse::<u64>().unwrap();
                    let s = s.replace(&caps[0], &encode_hex(ProgramId::from(id).as_slice()));
                    (s.clone().into_bytes()).to_vec()
                } else {
                    message.payload.clone().into_raw()
                }
            }
            _ => message.payload.clone().into_raw(),
        };
        runner.queue_message(
            message.destination.into(),
            payload,
            message.gas_limit,
        )
    }

    Ok(runner)
}

pub struct FinalState {
    pub log: Vec<Message>,
    pub allocation_storage: Vec<(PageNumber, ProgramId)>,
    pub program_storage: Vec<Program>,
}

pub fn run(
    mut runner: InMemoryRunner,
    steps: Option<u64>,
) -> anyhow::Result<(FinalState, Vec<u8>)> {
    if let Some(steps) = steps {
        for _ in 0..steps {
            runner.run_next()?;
        }
    } else {
        while runner.run_next()? > 0 {}
    }

    let (
        InMemoryStorage {
            message_queue,
            allocation_storage,
            program_storage,
        },
        persistent_memory,
    ) = runner.complete();
    // sort allocation_storage for tests
    let mut allocation_storage = allocation_storage.drain();
    allocation_storage.sort_by(|a, b| a.0.raw().partial_cmp(&b.0.raw()).unwrap());
    Ok((
        FinalState {
            log: message_queue.drain(),
            allocation_storage: allocation_storage,
            program_storage: program_storage.drain(),
        },
        persistent_memory,
    ))
}
