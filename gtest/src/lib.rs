mod log;
mod manager;
mod program;
mod system;

pub use log::{CoreLog, Log, RunResult};
pub use program::{Program, WasmProgram};
pub use system::System;

pub const EXISTENCE_DEPOSIT: u128 = 500;
