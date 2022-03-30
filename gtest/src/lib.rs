mod log;
mod manager;
mod program;
mod system;
mod mailbox;

pub use log::{CoreLog, Log, RunResult};
pub use program::{calculate_program_id, Program, WasmProgram};
pub use system::System;

pub const EXISTENTIAL_DEPOSIT: u128 = 500;
