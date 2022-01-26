mod log;
mod manager;
mod program;
mod system;

pub const DEFAULT_USER: u64 = 100001;

pub use log::{CoreLog, Log, RunResult};
pub use program::{Program, WasmProgram};
pub use system::System;
