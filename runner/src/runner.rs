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

use crate::sample::Test;

type InMemoryRunner =
    Runner<InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage>;

pub fn init_fixture(test: &Test, fixture_no: usize) -> anyhow::Result<InMemoryRunner> {
    let mut runner = Runner::new(
        &Config::default(),
        new_in_memory(Default::default(), Default::default(), Default::default()),
        &[],
    );
    for program in test.programs.iter() {
        let code = std::fs::read(program.path.clone())?.into();
        // let init_message = program.init_message.payload.raw().to_vec();
        let init_message = Vec::new(); // TODO: read also init message from test
        runner.init_program(program.id.into(), code, init_message)?;
    }

    let fixture = &test.fixtures[fixture_no];
    for message in fixture.messages.iter() {
        runner.queue_message(message.destination.into(), message.payload.raw().to_vec())
    }

    Ok(runner)
}

pub struct FinalState {
    pub log: Vec<Message>,
    pub allocation_storage: Vec<(PageNumber, ProgramId)>,
    program_storage: Vec<Program>,
    // TODO: keep allocations and such later for test fixtures inspection
}

pub fn run(mut runner: InMemoryRunner, steps: u64) -> anyhow::Result<FinalState> {
    for _ in 0..steps {
        runner.run_next()?;
    }

    let (
        InMemoryStorage {
            message_queue,
            allocation_storage,
            program_storage,
        },
        _,
    ) = runner.complete();
    // sort allocation_storage for tests
    let mut allocation_storage = allocation_storage.drain();
    allocation_storage.sort_by(|a, b| a.0.raw().partial_cmp(&b.0.raw()).unwrap());
    Ok(FinalState {
        log: message_queue.drain(),
        allocation_storage: allocation_storage,
        program_storage: program_storage.drain(),
    })
}
