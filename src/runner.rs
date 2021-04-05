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

use test_gear_sample::sample::Test;

type InMemoryRunner =
    Runner<InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage>;

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
            init_message = init_msg.clone().into_raw();
        }
        let mut gas_limit = u64::MAX;
        if let Some(limit) = program.gas_limit {
            gas_limit = limit;
        }
        runner.init_program(program.id.into(), code, init_message, gas_limit)?;
    }

    let fixture = &test.fixtures[fixture_no];
    for message in fixture.messages.iter() {
        runner.queue_message(
            message.destination.into(),
            message.payload.clone().into_raw(),
            u64::MAX
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
