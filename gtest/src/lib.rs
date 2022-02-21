mod log;
mod manager;
mod program;
mod system;

pub use log::{CoreLog, Log, RunResult};
pub use program::{Program, WasmProgram};
pub use system::System;

pub const EXISTENTIAL_DEPOSIT: u128 = 500;
