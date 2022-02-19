#![no_main]

use economic_checks::*;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|params: SimpleParams| {
    economic_checks::run_target(&Params::Simple(params), simple_scenario);
});
