//! Script to reproduce crashes found by `node-fuzzer`.
//!
//! This file is a temporary solution until #2313 is implemented.
//! Fuzzer dumps all the seed into the file, so the full run can
//! be reproduced in case of the fail.
//!
//! Just simply run `cargo run -- -p <path_to_fuzz_seeds>`.

use anyhow::{anyhow, Result};
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Params {
    /// Path to the file, which contains seeds from previously run fuzzer.
    #[structopt(short = "p", long, parse(from_os_str))]
    pub seeds_path: PathBuf,
}

fn main() -> Result<()> {
    gear_utils::init_default_logger();

    let mut file_reader = create_file_reader(Params::from_args().seeds_path)?;

    // Read and check seeds file header.
    let header = read_seeds_file_header(&mut file_reader)?;
    if !header.contains("Started fuzzing at") {
        return Err(anyhow!("Invalid seeds file format"));
    }

    // Read seeds and run test against all of them.
    for line in file_reader.lines() {
        let seed: u64 = line?.trim().parse()?;

        log::info!("Reproducing run with the seed - {seed}");

        node_fuzzer::run(seed);
    }

    Ok(())
}

fn create_file_reader(path: PathBuf) -> Result<BufReader<File>> {
    let file = File::open(path)?;

    Ok(BufReader::new(file))
}

fn read_seeds_file_header(file_reader: &mut BufReader<File>) -> Result<String> {
    let mut header = String::new();
    file_reader.read_line(&mut header)?;

    Ok(header)
}
