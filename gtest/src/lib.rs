mod ext;
mod log;
mod mailbox;
mod manager;
mod program;
mod system;

pub use ext::Ext;
pub use log::{CoreLog, Log, RunResult};
pub use program::{calculate_program_id, Program, WasmProgram};
pub use system::System;

pub const EXISTENTIAL_DEPOSIT: u128 = 500;
