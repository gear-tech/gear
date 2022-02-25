#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate economic_checks;

fuzz_target!(|params: economic_checks::Params| {
    println!("[sample_fuzz_target] data: {:?}", params);
    economic_checks::chain_of_multiplications(params);

    // Generate test contracts

    // Create a distribution of user accounts, mint funds

    // Prepare init messages I = V U IV, V - valid init messages (with valid wasm code), IV - invalid init messages

    // Deploy test contracts by sending out init messages on behalf of users

    // Mix ID's of deployed programs with a set of invalid ID's (in some ratio)

    // Generate messages to be sent to the network in some blocks in the future

    // for a block number in [2..N] send out messages and run queue processing

    // panic if invariants do not hold
});
