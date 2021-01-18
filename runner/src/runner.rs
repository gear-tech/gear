use gear_core::{
    message::Message,
    runner::{Runner, Config},
    storage::{new_in_memory, InMemoryStorage, InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage},
};

use crate::sample::Test;

type InMemoryRunner = Runner<InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage>;

pub fn init_fixture(test: &Test, fixture_no: usize) -> anyhow::Result<InMemoryRunner> {
    let mut runner = Runner::new(
        &Config::default(),
        new_in_memory(Default::default(), Default::default(), Default::default()),
        &[],
    );
    for program in test.programs.iter() {
        let code = std::fs::read(program.path.clone())?.into();
        let init_message = Vec::new(); // TODO: read also init message from test
        runner.init_program(program.id.into(), code, init_message)?;
    }

    let fixture = &test.fixtures[fixture_no];
    for message in fixture.messages.iter() {
        runner.queue_message(
            message.destination.into(),
            message.payload.raw().to_vec(),
        )
    }

    Ok(runner)
}

pub struct FinalState {
    log: Vec<Message>,
    // TODO: keep allocations and such later for test fixtures inspection
}

pub fn run(mut runner: InMemoryRunner) -> anyhow::Result<FinalState> {
    while runner.run_next()? > 0 {
    }

    let ( InMemoryStorage { message_queue, .. }, _) = runner.complete();

    Ok(FinalState {
        log: message_queue.log().iter().cloned().collect()
    })
}
