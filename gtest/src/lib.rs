mod log;
mod mailbox;
mod manager;
mod program;
mod system;
mod wasm_executor;

pub use log::{CoreLog, Log, RunResult};
pub use program::{calculate_program_id, Gas, Program, WasmProgram};
pub use system::System;

pub const EXISTENTIAL_DEPOSIT: u128 = 500;
