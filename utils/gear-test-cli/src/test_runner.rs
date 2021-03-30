use crate::sample::Test;
use common::*;
use rti::runner::ExtRunner;

use gear_core::{
    memory::PageNumber,
    message::Message,
    program::{Program, ProgramId},
    storage::{
        new_in_memory, InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage,
        InMemoryStorage, Storage
    },
};

use frame_system as system;

pub fn new_test_ext() -> sp_io::TestExternalities {
    system::GenesisConfig::default()
        .build_storage::<gear_runtime::Runtime>()
        .unwrap()
        .into()
}

pub fn init_fixture(test: &Test, fixture_no: usize) -> anyhow::Result<ExtRunner> {
    new_test_ext().execute_with(|| {
        // Dispatch a signed extrinsic.

        let mut runner = rti::runner::new();
        for program in test.programs.iter() {
            let code = std::fs::read(program.path.clone())?;
            let mut init_message = Vec::new();
            if let Some(init_msg) = &program.init_message {
                init_message = init_msg.clone().into_raw();
            }
            runner.init_program(program.id.into(), code, init_message)?;
        }
        let fixture = &test.fixtures[fixture_no];
        for message in fixture.messages.iter() {
            runner.queue_message(
                message.destination.into(),
                message.payload.clone().into_raw(),
            )
        }

        Ok(runner)
    })
}

pub struct FinalState {
    pub log: Vec<Message>,
    pub allocation_storage: Vec<(PageNumber, ProgramId)>,
    pub program_storage: Vec<Program>,
}

pub fn run(mut runner: ExtRunner, steps: Option<u64>) -> anyhow::Result<(FinalState, Vec<u8>)> {
    if let Some(steps) = steps {
        for _ in 0..steps {
            runner.run_next()?;
        }
    } else {
        while runner.run_next()? > 0 {}
    }

    // let (
    //     Storage<InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage> {
    //         message_queue,
    //         allocation_storage,
    //         program_storage,
    //     },
    //     persistent_memory,
    // ) = runner.complete();
    // // sort allocation_storage for tests
    // let mut allocation_storage = allocation_storage.drain();
    // allocation_storage.sort_by(|a, b| a.0.raw().partial_cmp(&b.0.raw()).unwrap());
    Ok((
        FinalState {
            log: Vec::new(),
            allocation_storage: Vec::new(),
            program_storage: Vec::new(),
        },
        Vec::new(),
    ))
}
