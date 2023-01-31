use demo_new_meta::MessageInitIn;
use gtest::{Program, System};

pub(super) fn initialize_current_program(system: &System) -> Program {
    const SOME_USER_ID: u64 = 3;
    let program = Program::current(system);
    program.send(
        SOME_USER_ID,
        MessageInitIn {
            amount: 123,
            currency: "USD".into(),
        },
    );
    program
}
