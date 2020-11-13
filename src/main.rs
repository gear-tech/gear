mod memory;
mod message;
mod program;

mod runner;
mod saver;

use anyhow::anyhow;

use message::Message;
use program::Program;

fn main() -> Result<(), anyhow::Error> {
    let file_name = std::env::args().nth(1).expect("gear <filename.wasm>");

    let program = Program::new(1.into(), std::fs::read(file_name)?.into(), vec![]);

    let mut runner = runner::Runner::new(
        vec![program],
        vec![],
        vec![Message { source: 0.into(), dest: 1.into(), payload: vec![].into() }],
        vec![],
    );

    runner.run_next()?;

    Ok(())
}
