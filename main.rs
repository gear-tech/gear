mod memory;
mod message;
mod program;

mod runner;
mod saver;

use std::path::PathBuf;

use anyhow::anyhow;

use message::Message;
use program::{Program, ProgramId};
use saver::State;

fn path() -> PathBuf {
    let mut path = PathBuf::from(directories::BaseDirs::new().expect("base path exist").home_dir());
    path.push(".gear");
    std::fs::create_dir_all(path.clone()).expect("Faield to create user dir");

    path.push("state.dat");

    path
}

fn main() -> Result<(), anyhow::Error> {
    let program_id_str = std::env::args().nth(1).expect("gear <pid> <filename.wasm>");
    let file_name = std::env::args().nth(2).expect("gear <pid> <filename.wasm>");

    let program_id: ProgramId = program_id_str.parse::<u64>().expect("gear <pid> <filename.wasm>").into();
    
    println!("Working state: {}", path().to_string_lossy());
    let mut state = saver::load_from_file(path());

    state.queued_messages.push(
        Message { source: 0.into(), dest: program_id, payload: vec![].into() }
    );

    let mut runner = state.into_runner();
    runner.update_program_code(
        program_id,
        std::fs::read(file_name)?.into(),
    );

    runner.run_next()?;

    let state = State::from_runner(runner);
    saver::save_to_file(path(), &state);

    Ok(())
}
