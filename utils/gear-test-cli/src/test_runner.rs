use crate::sample::Test;
use codec::Decode;
use rti::ext::{ExtAllocationStorage, ExtProgramStorage};
use rti::runner::ExtRunner;

use gear_core::{
    message::Message,
    storage::Storage,
};

use frame_system as system;

pub fn new_test_ext() -> sp_io::TestExternalities {
    system::GenesisConfig::default()
        .build_storage::<gear_runtime::Runtime>()
        .unwrap()
        .into()
}

pub fn init_fixture(
    ext: &mut sp_io::TestExternalities,
    test: &Test,
    fixture_no: usize,
) -> anyhow::Result<ExtRunner> {
    ext.execute_with(|| {
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
    pub message_queue: Vec<Message>,
    pub allocation_storage: ExtAllocationStorage,
    pub program_storage: ExtProgramStorage,
}

pub fn run(
    ext: &mut sp_io::TestExternalities,
    mut runner: ExtRunner,
    steps: Option<u64>,
) -> anyhow::Result<(FinalState, Vec<u8>)> {
    ext.execute_with(|| {
        if let Some(steps) = steps {
            for _ in 0..steps {
                runner.run_next()?;
            }
        } else {
            while runner.run_next()? > 0 {}
        }
        let message_queue = sp_io::storage::get(b"g::msg")
            .map(|val| Vec::<Message>::decode(&mut &val[..]).expect("values encoded correctly"))
            .unwrap_or_default();

        let (
            Storage {
                message_queue: _,
                allocation_storage,
                program_storage,
            },
            persistent_memory,
        ) = runner.complete();
        
        Ok((
            FinalState {
                message_queue,
                allocation_storage,
                program_storage,
            },
            persistent_memory,
        ))
    })
}
