#![no_main]

use libfuzzer_sys::fuzz_target;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
};

const SEEDS_STORE: &str = "fuzzing_seeds";

fuzz_target!(|seed: u64| {
    gear_utils::init_default_logger();

    dump_seed(seed).expect("internal error: failed dumping seed");

    log::info!("Running the seed {seed}");
    node_fuzzer::run(seed);
});

// Dumps seed to the file before running fuzz test.
//
// Puts in the beginning the timestamp string if file is new.
fn dump_seed(seed: u64) -> Result<(), String> {
    let is_new_file = !Path::new(SEEDS_STORE).exists();
    let dump_timestamp_if_new = |file: &mut File| {
        if is_new_file {
            writeln!(file, "Started fuzzing at {}", gear_utils::now_millis())
        } else {
            Ok(())
        }
    };

    OpenOptions::new()
        .create(true)
        .append(true)
        .open(SEEDS_STORE)
        .map_err(|e| e.to_string())
        .and_then(|mut file| {
            dump_timestamp_if_new(&mut file)
                .and_then(|_| writeln!(file, "{seed}"))
                .map_err(|e| e.to_string())
        })
}
